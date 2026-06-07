import { extractTriggerScripts, runTriggers, createTriggerContext, TriggerType } from "./triggerEngine.js";
import { getSelectedPersonaPrompt } from "./appStore.svelte.js";

/** Parse toggle definitions from preset's customPromptTemplateToggle string.
 *  Format: one per line, `key=Label` or `key:Label` (matching the regex
 *  already used in assemblePrompt).  Returns array of {key, label}. */
export function parseToggleDefs(preset) {
  if (!preset?.customPromptTemplateToggle) return [];
  return preset.customPromptTemplateToggle.split("\n")
    .map(line => {
      const m = line.match(/^(\w+)\s*[:=]\s*(.+)$/);
      return m ? { key: m[1], label: m[2].trim() } : null;
    })
    .filter(Boolean);
}

/** Initialise activeToggles for the given defs, preserving any existing
 *  state (e.g. from a restored session) and defaulting new keys to ON.
 *  Returns the new toggles object. */
export function initToggles(defs, currentToggles) {
  const next = {};
  for (const d of defs) {
    next[d.key] = currentToggles[d.key] !== undefined ? currentToggles[d.key] : true;
  }
  return next;
}

/** Embed a user query for RAG retrieval.
 *  @param {Function} invoke - Tauri invoke function
 *  @param {Function} flashError - error display callback
 *  @param {object} settings - app settings
 *  @param {string} text - query text to embed */
export async function embedQueryForRag(invoke, flashError, settings, text) {
  const h = settings.hypa || {};
  const provider = h.embeddingProvider || "vertex";
  const model = h.embeddingModel || "gemini-embedding-2";
  try {
    if (provider === "vertex") {
      const result = await invoke("embed_vertex", {
        texts: [text],
        model,
        project_id: settings.vertexProjectId || "",
        region: settings.vertexRegion || "us-central1",
      });
      return Array.isArray(result?.[0]) ? result[0] : null;
    } else if (provider === "voyage") {
      const result = await invoke("embed_voyage", {
        texts: [text],
        model,
        api_key: settings.voyageKey || "",
      });
      return Array.isArray(result?.[0]) ? result[0] : null;
    }
  } catch (e) {
    console.error("embed for RAG failed:", e);
    flashError(e, "임베딩");
  }
  return null;
}

/** Check whether a lorebook entry should be activated given conversation text.
 *  @param {object} entry - lorebook entry with keys, secondkey, selective, alwaysActive, activationPercent, disable
 *  @param {string} conversationText - recent conversation text to match against
 *  @returns {boolean} */
export function matchesLorebook(entry, conversationText) {
  if (entry.disable) return false;
  if (entry.alwaysActive || entry.always_active) return true;

  const keys = Array.isArray(entry.keys)
    ? entry.keys
    : (entry.key || "").split(",").map(k => k.trim());
  const primaryMatch = keys.some(k => k && conversationText.includes(k));

  if (entry.selective && entry.secondkey) {
    const secKeys = (typeof entry.secondkey === "string" ? entry.secondkey : "")
      .split(",").map(k => k.trim());
    const secondaryMatch = secKeys.some(k => k && conversationText.includes(k));
    return primaryMatch && secondaryMatch;
  }

  if (!primaryMatch && typeof entry.activationPercent === "number" && entry.activationPercent > 0) {
    return Math.random() * 100 < entry.activationPercent;
  }

  return primaryMatch;
}

/** Assemble the system prompt and post-chat text from preset + character.
 *  Returns { systemPrompt, postChatText, loadedPreset, toggleDefs, activeToggles, triggerScripts }.
 *
 *  @param {Function} invoke - Tauri invoke function
 *  @param {Function} flashError - error display callback
 *  @param {object} opts
 *  @param {object|null} opts.loadedCharacter
 *  @param {object} opts.settings
 *  @param {object} opts.activeToggles - current toggle state
 *  @param {string} [opts.conversationText] - recent conversation text for lorebook keyword matching
 *  @param {number[]|null} [opts.queryEmbedding] - precomputed query embedding for the HyPA memory slot's similar phase (omit/empty → that phase is skipped)
 *  @param {object|null} [opts.cachedPreset] - pre-loaded preset from appStore (skips invoke if provided)
 *  @param {string|null} [opts.personaPrompt] - selected persona prompt (overrides character persona slot) */
/** Default share of the context window given to the HyPA memory slot when the
 *  preset/settings do not specify `memoryTokensRatio`. Matches the backend
 *  `HypaSettings::default().memory_tokens_ratio` (0.2) so the slot budget is
 *  consistent across the JS and Rust sides. */
const DEFAULT_MEMORY_TOKENS_RATIO = 0.2;

/** Token budget for the memory slot when the preset declares no context limit
 *  (maxContext <= 0 / -1000, the "unlimited" sentinel in trimToContextWindow).
 *  A fixed cap so unlimited-context presets still bound the memory slot. */
const NO_LIMIT_MEMORY_TOKEN_BUDGET = 8192;

/** Translate the frontend `settings.hypa` object onto the backend HypaSettings
 *  camelCase shape. Only keys the frontend currently persists are mapped; the
 *  backend fills every other field from its own defaults (tolerant deserialize),
 *  so omissions are safe. `similarRatio` is the frontend's internal name for the
 *  external `similarMemoryRatio` key (see HypaView import mapping). */
function hypaSettingsForBackend(hypa) {
  const out = {};
  if (hypa?.similarRatio != null) out.similarMemoryRatio = hypa.similarRatio;
  if (hypa?.memoryTokensRatio != null) out.memoryTokensRatio = hypa.memoryTokensRatio;
  if (hypa?.recentMemoryRatio != null) out.recentMemoryRatio = hypa.recentMemoryRatio;
  if (hypa?.maxChatsPerSummary != null) out.maxChatsPerSummary = hypa.maxChatsPerSummary;
  if (hypa?.preserveOrphanedMemory != null) out.preserveOrphanedMemory = hypa.preserveOrphanedMemory;
  return out;
}

export async function assemblePrompt(invoke, flashError, { loadedCharacter, settings, activeToggles, conversationText, queryEmbedding, cachedPreset, personaPrompt }) {
  let systemPrompt = null;
  let postChatText = "";
  let loadedPreset = null;
  let toggleDefs = [];
  let triggerScripts = [];
  let newActiveToggles = { ...activeToggles };

  try {
    const preset = cachedPreset || await invoke("cmd_load_current_preset");
    loadedPreset = preset;
    if (preset?.promptTemplate) {
      const userName = settings.userName || "User";
      let charName = "Character";
      let characterDesc = "";
      let characterPersona = "";
      let lorebook = [];
      {
        const card = loadedCharacter?.card?.data || loadedCharacter?.card || {};
        charName = card.name || "Character";
        characterDesc = card.description || "";
        characterPersona = card.personality || "";
        const ext = card.extensions?.risuai || {};
        if (ext.additionalData?.lorebook) {
          lorebook = ext.additionalData.lorebook.filter(e => !e.disable);
        } else if (card.character_book?.entries) {
          lorebook = Object.values(card.character_book.entries).filter(e => e.enabled !== false);
        }
      }

      // Extract trigger scripts from character card
      triggerScripts = extractTriggerScripts(loadedCharacter);

      const regexList = [...(preset.regex || [])];

      // Parse toggle definitions and refresh UI state
      const defs = parseToggleDefs(preset);
      if (defs.length > 0) {
        toggleDefs = defs;
        newActiveToggles = initToggles(defs, activeToggles);
      } else {
        toggleDefs = [];
      }

      // Build toggles map: only include entries the user has enabled.
      const toggles = {};
      if (preset.customPromptTemplateToggle) {
        for (const line of preset.customPromptTemplateToggle.split("\n")) {
          const m = line.match(/^(\w+)\s*[:=]\s*(.+)$/);
          if (m) {
            const key = m[1];
            if (newActiveToggles[key] !== false) {
              toggles[key] = m[2].trim();
            }
          }
        }
      }
      if (preset.templateDefaultVariables) {
        for (const line of preset.templateDefaultVariables.split("\n")) {
          const m = line.match(/^(\w+)\s*[:=]\s*(.+)$/);
          if (m) toggles[m[1]] = m[2].trim();
        }
      }

      // Merge module lorebook entries into character lorebook
      try {
        if (loadedCharacter?.moduleData) {
          const modData = typeof loadedCharacter.moduleData === "string" ? JSON.parse(loadedCharacter.moduleData) : loadedCharacter.moduleData;
          const modLorebook = modData?.lorebook || modData?.data?.lorebook || [];
          const active = (Array.isArray(modLorebook) ? modLorebook : []).filter(e => !e.disable);
          lorebook = [...lorebook, ...active];
        }
      } catch (e) { console.warn("module lorebook parse:", e); }

      // Merge enabled modules (multi-module support)
      try {
        const enabledModuleIds = settings.enabledModules || [];
        if (enabledModuleIds.length > 0) {
          const loadedModules = await invoke("cmd_load_modules", { ids: enabledModuleIds });
          for (const mod of loadedModules) {
            const modObj = mod?.data || mod || {};
            // Merge lorebook entries
            const modLorebook = modObj.lorebook || [];
            const activeEntries = (Array.isArray(modLorebook) ? modLorebook : []).filter(e => !e.disable);
            lorebook = [...lorebook, ...activeEntries];
            // Merge regex entries
            const modRegex = modObj.regex || [];
            if (Array.isArray(modRegex) && modRegex.length > 0) {
              regexList.push(...modRegex);
            }
            // Merge custom module toggles
            const modToggles = modObj.customModuleToggle;
            if (typeof modToggles === "string" && modToggles.trim()) {
              for (const line of modToggles.split("\n")) {
                const m = line.match(/^(\w+)\s*[:=]\s*(.+)$/);
                if (m) toggles[m[1]] = m[2].trim();
              }
            }
          }
        }
      } catch (e) {
        console.error("Multi-module merge failed:", e);
        flashError(e, "모듈 로드");
      }

      const preChatParts = [];
      const postChatParts = [];
      let chatInserted = false;
      const emit = (text) => {
        if (chatInserted) postChatParts.push(text);
        else preChatParts.push(text);
      };

      for (const item of preset.promptTemplate) {
        if (!item) continue;
        const type = item.type || "";

        if (type === "plain" || type === "jailbreak" || type === "cot") {
          let text = item.data || item.text || "";
          if (text.trim()) {
            try { text = await invoke("evaluate_cbs", { input: text, char_name: charName, user_name: userName, toggles }); } catch (e) { console.warn("evaluate_cbs:", e); }
            for (const rx of regexList) {
              if (rx.type === "editinput" || rx.type === "editoutput") continue;
              try { text = text.replace(new RegExp(rx.in, rx.flag || "g"), rx.out || ""); } catch (e) { console.warn("regex replace failed:", rx.in, e); }
            }
            if (text.trim()) emit(text);
          }
        } else if (type === "description") {
          const fmt = item.innerFormat || "{{slot}}";
          const desc = characterDesc || "";
          if (desc || fmt !== "{{slot}}") emit(fmt.replace(/\{\{slot\}\}/g, desc));
        } else if (type === "persona") {
          const fmt = item.innerFormat || "{{slot}}";
          let personaSlot = characterPersona || userName;
          const resolvedPersona = personaPrompt || getSelectedPersonaPrompt();
          if (resolvedPersona) {
            try {
              personaSlot = await invoke("evaluate_cbs", { input: resolvedPersona, char_name: charName, user_name: userName, toggles });
            } catch (e) {
              console.warn("evaluate_cbs for persona:", e);
              personaSlot = resolvedPersona;
            }
          }
          emit(fmt.replace(/\{\{slot\}\}/g, personaSlot));
        } else if (type === "memory") {
          // HyPA memory slot. Routed through the core 4-phase select_memories
          // pipeline (hypa_select_block) instead of dumping every summary:
          // important → pinned → recent → similar → random, budgeted to a share
          // of the context window. queryEmbedding (passed by ChatView from its
          // precomputed userMsg embedding) drives the similar phase; when absent
          // the backend skips that phase and still runs the others (graceful
          // degradation), so non-chat callers still get important/recent memory.
          const fmt = item.innerFormat || "{{slot}}";
          let memoryBlock = "";
          const hypa = settings.hypa || {};
          // hypa_select_block wraps the selected summaries in <Past Events
          // Summary> after selection. The backend reserves the wrapper's own
          // token cost before selecting: it subtracts estimate_tokens(
          // MEMORY_BLOCK_WRAPPER) from this tokenBudget (saturating_sub) so the
          // wrapped block fits the budget we pass here. See MEMORY_BLOCK_WRAPPER
          // in commands_hypa.rs (hypa_select_block).
          const ratio = hypa.memoryTokensRatio ?? DEFAULT_MEMORY_TOKENS_RATIO;
          const maxContext = loadedPreset?.maxContext;
          // maxContext <= 0 / -1000 means "no limit" (see trimToContextWindow);
          // fall back to a generous fixed budget so memory still selects.
          const tokenBudget = (maxContext && maxContext > 0)
            ? Math.floor(maxContext * ratio)
            : NO_LIMIT_MEMORY_TOKEN_BUDGET;
          try {
            const queryEmbeddings = Array.isArray(queryEmbedding) && queryEmbedding.length > 0
              ? [queryEmbedding]
              : [];
            const queryWeights = queryEmbeddings.length > 0 ? [1.0] : [];
            memoryBlock = await invoke("hypa_select_block", {
              query_embeddings: queryEmbeddings,
              query_weights: queryWeights,
              token_budget: tokenBudget,
              settings: hypaSettingsForBackend(hypa),
            });
            if (typeof memoryBlock !== "string") memoryBlock = "";
          } catch (e) {
            console.error("memory slot select failed:", e);
            flashError(e, "메모리 선택");
          }
          if (memoryBlock || fmt !== "{{slot}}") emit(fmt.replace(/\{\{slot\}\}/g, memoryBlock));
        } else if (type === "lorebook") {
          // Filter by keyword matching when conversationText is available;
          // otherwise fall back to injecting all non-disabled entries (prior behavior).
          const activated = conversationText
            ? lorebook.filter(e => matchesLorebook(e, conversationText))
            : lorebook;

          // Sort by insertorder (lower = earlier); stable within same order.
          const sorted = [...activated].sort((a, b) => {
            const orderA = a.insertorder ?? a.insertOrder ?? 100;
            const orderB = b.insertorder ?? b.insertOrder ?? 100;
            return orderA - orderB;
          });

          for (const entry of sorted) {
            const content = entry.content || entry.value || "";
            const key = Array.isArray(entry.keys) ? entry.keys.join(", ") : (entry.key || "");
            if (content.trim()) emit(`[lorebook: ${key}] ${content}`);
          }
        } else if (type === "chat") {
          chatInserted = true;
        } else if (type === "authornote") {
          const fmt = item.innerFormat || item.defaultText || "";
          if (fmt.trim()) emit(fmt);
        } else if (type === "postEverything") {
          const fmt = item.innerFormat || "";
          if (fmt.trim()) emit(fmt);
        }
      }

      // No chat cut -> preserve prior behavior: everything is system.
      // With cut -> preChat is system; postChat travels with the last user msg.
      if (chatInserted) {
        if (preChatParts.length > 0) systemPrompt = preChatParts.join("\n\n");
        if (postChatParts.length > 0) postChatText = postChatParts.join("\n\n");
      } else if (preChatParts.length > 0) {
        systemPrompt = preChatParts.join("\n\n");
      }
    }
  } catch (e) {
    console.error("assemblePrompt failed:", e);
    flashError(e, "프롬프트 조립");
  }

  return { systemPrompt, postChatText, loadedPreset, toggleDefs, activeToggles: newActiveToggles, triggerScripts };
}

/** Apply editdisplay triggers to text before rendering.
 *  @param {object[]} triggerScripts - trigger scripts from character card
 *  @param {string} text - text to transform
 *  @param {Record<string, string>} triggerVars - session trigger variables
 *  @returns {{ text: string, variables: Record<string, string> }} */
export function applyEditDisplayTriggers(triggerScripts, text, triggerVars) {
  if (!Array.isArray(triggerScripts) || triggerScripts.length === 0 || !text) {
    return { text, variables: triggerVars || {} };
  }
  const ctx = createTriggerContext(triggerVars || {}, { text });
  runTriggers(triggerScripts, TriggerType.EDIT_DISPLAY, ctx);
  return { text: ctx.text ?? text, variables: ctx.variables };
}

/** Apply editinput triggers to the user message before sending.
 *  @param {object[]} triggerScripts - trigger scripts from character card
 *  @param {string} text - user message text
 *  @param {Record<string, string>} triggerVars - session trigger variables
 *  @param {object} [opts] - { chatIndex, recentMessages }
 *  @returns {{ text: string, variables: Record<string, string> }} */
export function applyEditInputTriggers(triggerScripts, text, triggerVars, opts = {}) {
  if (!Array.isArray(triggerScripts) || triggerScripts.length === 0 || !text) {
    return { text, variables: triggerVars || {} };
  }
  const ctx = createTriggerContext(triggerVars || {}, {
    text,
    chatIndex: opts.chatIndex,
    recentMessages: opts.recentMessages,
  });
  runTriggers(triggerScripts, TriggerType.EDIT_INPUT, ctx);
  return { text: ctx.text ?? text, variables: ctx.variables };
}

/** Apply editoutput triggers to the AI response after receiving.
 *  @param {object[]} triggerScripts - trigger scripts from character card
 *  @param {string} text - AI response text
 *  @param {Record<string, string>} triggerVars - session trigger variables
 *  @param {object} [opts] - { chatIndex, recentMessages }
 *  @returns {{ text: string, variables: Record<string, string> }} */
export function applyEditOutputTriggers(triggerScripts, text, triggerVars, opts = {}) {
  if (!Array.isArray(triggerScripts) || triggerScripts.length === 0 || !text) {
    return { text, variables: triggerVars || {} };
  }
  const ctx = createTriggerContext(triggerVars || {}, {
    text,
    chatIndex: opts.chatIndex,
    recentMessages: opts.recentMessages,
  });
  runTriggers(triggerScripts, TriggerType.EDIT_OUTPUT, ctx);
  return { text: ctx.text ?? text, variables: ctx.variables };
}
