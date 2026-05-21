<script>
  import { invoke, isTauri } from "../lib/tauri.js";
  import { onMount, onDestroy } from "svelte";

  // Parent registers our public interface (sendChatMessage, getMessages)
  // so AutopilotView can drive chat without ChatView being its parent.
  // hypaApi: optional, when present we auto-summarize and inject RAG context.
  let { onReady, hypaApi } = $props();

  let messages = $state([]);
  let chatInput = $state("");
  let streaming = $state(false);
  let sessionLoaded = $state(false);
  let streamingText = $state("");
  let greetings = $state([]);
  let greetingIndex = $state(0);
  // Sentinel scrolled into view on new messages — keeps the latest message
  // visible regardless of which ancestor element actually owns the scroll.
  let chatBottom;
  let unlisten;
  let unlistenAppFlush;
  let sessionSaveTimeout;
  let editingMsgId = $state(null);
  let editingText = $state("");
  let error = $state("");
  let chatProvider = $state("");   // active provider label for indicator
  let chatInputEl;                 // ref for auto-focus

  // ── Fork (branching) state ──────────────────────────────────────────
  // Git branching model:
  //   message = commit (chatId = commit hash)
  //   linear conversation = main branch
  //   fork = new branch from a specific commit (forkPoint)
  //   each branch has its own head (latest message)
  //   branch switching = git checkout (activeBranchId change)
  //
  // Session persists: messages[], branches[], activeBranchId
  // Each message: { chatId, parentId, branchId, role, text, time, ... }
  // Each branch:  { id, name, headId, forkPoint }
  const MAIN_BRANCH_ID = "main";
  let branches = $state([{ id: MAIN_BRANCH_ID, name: "main", headId: null, forkPoint: null }]);
  let activeBranchId = $state(MAIN_BRANCH_ID);
  // UI: which message's fork dropdown is open
  let forkDropdownId = $state(null);

  // Preset toggle UI state — keys from customPromptTemplateToggle, values
  // are booleans. Defaults to all-ON so existing behaviour is preserved
  // when a session has no saved toggle state.
  let activeToggles = $state({});   // { dark: true, verbose: false, … }
  let toggleDefs = $state([]);      // [{ key: "dark", label: "Dark Mode" }, …]
  let togglePanelOpen = $state(false);
  const ERROR_CLEAR_MS = 3000;
  function flashError(msg) {
    error = msg;
    setTimeout(() => { if (error === msg) error = ""; }, ERROR_CLEAR_MS);
  }

  /** Parse toggle definitions from preset's customPromptTemplateToggle string.
   *  Format: one per line, `key=Label` or `key:Label` (matching the regex
   *  already used in assemblePrompt).  Returns array of {key, label}. */
  function parseToggleDefs(preset) {
    if (!preset?.customPromptTemplateToggle) return [];
    return preset.customPromptTemplateToggle.split("\n")
      .map(line => {
        const m = line.match(/^(\w+)\s*[:=]\s*(.+)$/);
        return m ? { key: m[1], label: m[2].trim() } : null;
      })
      .filter(Boolean);
  }

  /** Initialise activeToggles for the given defs, preserving any existing
   *  state (e.g. from a restored session) and defaulting new keys to ON. */
  function initToggles(defs) {
    const next = {};
    for (const d of defs) {
      next[d.key] = activeToggles[d.key] !== undefined ? activeToggles[d.key] : true;
    }
    activeToggles = next;
  }

  // Stable id for each message — used by HyPA Summary.chatMemos to link
  // summaries back to the originating messages (and by hypa_pin_message /
  // hypa_invalidate_summary cascades). crypto.randomUUID is available on
  // every modern browser/WebView; fallback covers older Android WebViews.
  function newChatId() {
    if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
      return crypto.randomUUID();
    }
    return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
  }

  // ── Fork pure functions ─────────────────────────────────────────────
  // These operate on the flat messages array + branches metadata to
  // produce the visible message chain for a given branch.

  /** Migrate legacy sessions: messages without parentId/branchId are treated
   *  as a linear "main" chain. Assigns parentId from array order and
   *  branchId = MAIN_BRANCH_ID. Returns { messages, branches, activeBranchId }. */
  function migrateSession(msgs, savedBranches, savedActiveBranchId) {
    if (!Array.isArray(msgs) || msgs.length === 0) {
      return {
        messages: [],
        branches: [{ id: MAIN_BRANCH_ID, name: "main", headId: null, forkPoint: null }],
        activeBranchId: MAIN_BRANCH_ID,
      };
    }

    // If first message already has branchId, session is already migrated
    if (msgs[0].branchId) {
      return {
        messages: msgs,
        branches: savedBranches && savedBranches.length > 0
          ? savedBranches
          : [{ id: MAIN_BRANCH_ID, name: "main", headId: msgs[msgs.length - 1].chatId, forkPoint: null }],
        activeBranchId: savedActiveBranchId || MAIN_BRANCH_ID,
      };
    }

    // Legacy migration: assign parentId chain and branchId
    const migrated = msgs.map((m, i) => ({
      ...m,
      parentId: i === 0 ? null : msgs[i - 1].chatId,
      branchId: MAIN_BRANCH_ID,
    }));

    const headId = migrated.length > 0 ? migrated[migrated.length - 1].chatId : null;
    return {
      messages: migrated,
      branches: [{ id: MAIN_BRANCH_ID, name: "main", headId, forkPoint: null }],
      activeBranchId: MAIN_BRANCH_ID,
    };
  }

  /** Build a lookup from chatId to message for O(1) access. */
  function buildIndex(msgs) {
    const idx = new Map();
    for (const m of msgs) idx.set(m.chatId, m);
    return idx;
  }

  /** Walk from a leaf message back to root via parentId chain.
   *  Returns messages in root-to-leaf order (conversation order). */
  function getChainToRoot(msgs, leafId) {
    const idx = buildIndex(msgs);
    const chain = [];
    let current = leafId;
    // Safety: max iterations to prevent infinite loops from corrupt data
    let safety = msgs.length + 1;
    while (current && safety-- > 0) {
      const msg = idx.get(current);
      if (!msg) break;
      chain.push(msg);
      current = msg.parentId;
    }
    chain.reverse();
    return chain;
  }

  /** Get the visible messages for the active branch.
   *  Pure function: messages array + branches + activeBranchId → visible messages.
   *  Walks from the branch's head back to root via parentId. */
  function getVisibleMessages(msgs, branchList, activeId) {
    const branch = branchList.find(b => b.id === activeId);
    if (!branch || !branch.headId) return [];
    return getChainToRoot(msgs, branch.headId);
  }

  /** Count how many child branches fork from a given message chatId. */
  function countForks(branchList, chatId) {
    return branchList.filter(b => b.forkPoint === chatId).length;
  }

  /** Get branches that fork from a given message chatId. */
  function getBranchesAtForkPoint(branchList, chatId) {
    return branchList.filter(b => b.forkPoint === chatId);
  }

  /** Update the head of a branch. Returns new branches array. */
  function updateBranchHead(branchList, branchId, newHeadId) {
    return branchList.map(b =>
      b.id === branchId ? { ...b, headId: newHeadId } : b
    );
  }

  /** Create a new branch forking from forkPointId.
   *  Returns { newBranch, branches: updatedBranchesList }. */
  function createForkBranch(branchList, forkPointId, name) {
    const id = newChatId();
    const newBranch = { id, name, headId: forkPointId, forkPoint: forkPointId };
    return { newBranch, branches: [...branchList, newBranch] };
  }

  /** Append a message to the flat array and update the branch head.
   *  parentId is the current head of the branch. Returns { messages, branches, newMsg }. */
  function appendMessage(msgs, branchList, branchId, role, text, time, extraFields) {
    const branch = branchList.find(b => b.id === branchId);
    const parentId = branch?.headId || null;
    const chatId = newChatId();
    const newMsg = { chatId, parentId, branchId, role, text, time, ...(extraFields || {}) };
    const newMsgs = [...msgs, newMsg];
    const newBranches = updateBranchHead(branchList, branchId, chatId);
    return { messages: newMsgs, branches: newBranches, newMsg };
  }

  // ── Derived state ───────────────────────────────────────────────────
  // visibleMessages is computed from messages + branches + activeBranchId.
  let visibleMessages = $derived(getVisibleMessages(messages, branches, activeBranchId));

  // ── Session serialization helpers ───────────────────────────────────
  function sessionPayload() {
    return { messages, activeToggles, branches, activeBranchId };
  }

  onMount(async () => {
    if (isTauri()) {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        unlisten = await listen("chat-chunk", (event) => {
          streamingText += event.payload;
        });
        // app-flush: App.svelte broadcasts this on close-requested; we save
        // immediately rather than waiting for the 1000ms session debounce
        // (which the window-destroy timer would race past). Without this,
        // the 500ms grace in App.svelte is just dead time.
        unlistenAppFlush = await listen("app-flush", async () => {
          clearTimeout(sessionSaveTimeout);
          if (sessionLoaded) {
            try {
              await invoke("cmd_save_session", { session: sessionPayload() });
            } catch (e) { console.error("ChatView app-flush save failed:", e); }
          }
        });
      } catch (e) {
        console.error("Event listen failed:", e);
        flashError("Event listener setup failed — streaming may not work");
      }
    }
    // Load persisted chat session (with backward-compatible migration)
    try {
      const savedSession = await invoke("cmd_load_session");
      if (savedSession?.messages?.length && messages.length === 0) {
        const migrated = migrateSession(
          savedSession.messages,
          savedSession.branches,
          savedSession.activeBranchId,
        );
        messages = migrated.messages;
        branches = migrated.branches;
        activeBranchId = migrated.activeBranchId;
      }
      if (savedSession?.activeToggles && typeof savedSession.activeToggles === "object") {
        activeToggles = savedSession.activeToggles;
      }
    } catch (e) {
      console.error("Failed to load session:", e);
      flashError("Failed to load previous session");
    }
    sessionLoaded = true;

    // Load provider label for indicator
    loadProviderLabel();

    // Expose interface to parent — getMessages returns visible branch only
    onReady?.({
      sendChatMessage,
      getMessages: () => visibleMessages,
    });
  });

  onDestroy(() => {
    if (unlisten) unlisten();
    if (unlistenAppFlush) unlistenAppFlush();
    // Flush pending session save immediately before clearing timeout.
    if (sessionLoaded && messages.length > 0) {
      invoke("cmd_save_session", { session: sessionPayload() }).catch(e => console.error("session save on destroy failed:", e));
    }
    clearTimeout(sessionSaveTimeout);
  });

  $effect(() => {
    messages;
    streamingText;
    if (chatBottom) {
      requestAnimationFrame(() => {
        chatBottom.scrollIntoView({ block: "end", behavior: "smooth" });
      });
    }
  });

  $effect(() => {
    const msgCount = messages.length;
    // Read reactive deps so the effect re-runs when they change.
    const _toggleSnapshot = activeToggles;
    const _branchSnapshot = branches;
    const _activeBranchSnapshot = activeBranchId;
    if (sessionLoaded && msgCount > 0) {
      clearTimeout(sessionSaveTimeout);
      sessionSaveTimeout = setTimeout(async () => {
        try {
          await invoke("cmd_save_session", { session: sessionPayload() });
        } catch (e) {
          console.error("Session save failed:", e);
          flashError(`Session save failed: ${e}`);
        }
      }, 1000);
    }
  });

  async function embedQueryForRag(settings, text) {
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
      flashError("Embedding failed — sending without RAG context");
    }
    return null;
  }

  async function sendChatMessage(userMsg) {
    // first_mes auto-insertion: when starting from an empty session, seed
    // the chat history with the character's greeting (role: "char", matching
    // Layream's stored convention — see msgs.map below mapping char→model).
    // Load character once — reused for first_mes, prompt assembly, and module lorebook.
    let loadedCharacter = null;
    try {
      loadedCharacter = await invoke("cmd_load_current_character");
    } catch (e) { console.error("cmd_load_current_character failed:", e); }

    if (messages.length === 0 && loadedCharacter) {
      try {
        const card = loadedCharacter?.card?.data || loadedCharacter?.card || {};
        const firstMes = card.first_mes || "";
        const altGreetings = card.alternate_greetings || card.alternateGreetings || [];
        const allGreetings = [firstMes, ...altGreetings].filter(g => g && g.trim());
        if (allGreetings.length > 0) {
          greetings = allGreetings;
          greetingIndex = 0;
          const result = appendMessage(messages, branches, activeBranchId, "char", allGreetings[0], new Date().toLocaleTimeString());
          messages = result.messages;
          branches = result.branches;
        }
      } catch (e) { console.error("first_mes load failed:", e); }
    }

    // Append user message to the active branch
    const userResult = appendMessage(messages, branches, activeBranchId, "user", userMsg, new Date().toLocaleTimeString());
    messages = userResult.messages;
    branches = userResult.branches;
    streaming = true;
    streamingText = "";

    let pollInterval = setInterval(async () => {
      try {
        const chunks = await invoke("poll_stream_chunks");
        if (chunks?.length) {
          for (const chunk of chunks) streamingText += chunk;
        }
      } catch (e) { console.warn("poll_stream_chunks:", e); }
    }, 100);

    try {
      await invoke("start_streaming", { text: "AI 응답 수신 중..." }).catch(e => console.error("start_streaming failed:", e));

      const settings = await invoke("cmd_load_settings") || {};
      const provider = settings.chatProvider || "vertex";

      // Assemble system prompt from preset + character (ref: RisuExtractUtil test-view.ts:397-436).
      // chat type acts as a cut point: parts before it become system_instruction;
      // parts after it must be injected after chat history to preserve their
      // intended position relative to the conversation (§2-D non-commutative).
      let systemPrompt = null;
      let postChatText = "";
      let loadedPreset = null;
      try {
        const preset = await invoke("cmd_load_current_preset");
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

          const regexList = [...(preset.regex || [])];

          // Parse toggle definitions and refresh UI state
          const defs = parseToggleDefs(preset);
          if (defs.length > 0) {
            toggleDefs = defs;
            initToggles(defs);
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
                if (activeToggles[key] !== false) {
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
            flashError("Failed to load enabled modules — continuing without them");
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
              emit(fmt.replace(/\{\{slot\}\}/g, characterPersona || userName));
            } else if (type === "memory") {
              const fmt = item.innerFormat || "{{slot}}";
              let memoryBlock = "";
              if (hypaApi?.loadAll) {
                try {
                  const allSummaries = await hypaApi.loadAll();
                  if (Array.isArray(allSummaries) && allSummaries.length > 0) {
                    memoryBlock = allSummaries.map(s => s?.text || s?.content || "").filter(Boolean).join("\n\n");
                  }
                } catch (e) {
                  console.error("memory slot load failed:", e);
                  flashError("Failed to load memory for prompt");
                }
              }
              if (memoryBlock || fmt !== "{{slot}}") emit(fmt.replace(/\{\{slot\}\}/g, memoryBlock));
            } else if (type === "lorebook") {
              for (const entry of lorebook) {
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

          // No chat cut → preserve prior behavior: everything is system.
          // With cut → preChat is system; postChat travels with the last user msg.
          if (chatInserted) {
            if (preChatParts.length > 0) systemPrompt = preChatParts.join("\n\n");
            if (postChatParts.length > 0) postChatText = postChatParts.join("\n\n");
          } else if (preChatParts.length > 0) {
            systemPrompt = preChatParts.join("\n\n");
          }
        }
      } catch (e) {
        console.error("assemblePrompt failed:", e);
        flashError("Preset/prompt assembly failed — sending without system prompt");
      }

      // RAG: retrieve relevant summaries, inject as a memory header into the user message
      let injectedUserMsg = userMsg;
      if (hypaApi?.getRagContext) {
        try {
          const queryEmbedding = await embedQueryForRag(settings, userMsg);
          if (queryEmbedding) {
            const hits = await hypaApi.getRagContext(queryEmbedding, 5);
            if (Array.isArray(hits) && hits.length > 0) {
              const memoryText = hits
                .map(h => h?.summary?.text)
                .filter(Boolean)
                .join("\n\n");
              if (memoryText) {
                injectedUserMsg = `[Memory]\n${memoryText}\n\n[User]\n${userMsg}`;
              }
            }
          }
        } catch (e) {
          console.error("RAG injection failed:", e);
          flashError("RAG context retrieval failed — sending without memory");
        }
      }

      // Use visible messages (active branch chain) for AI context —
      // other branches must not leak into the prompt.
      let msgs = visibleMessages.filter(m => m.role !== "error").map((m, idx, arr) => ({
        role: m.role === "char" ? "model" : m.role,
        text: idx === arr.length - 1 && m.role === "user" ? injectedUserMsg : m.text,
      }));

      const maxCtx = loadedPreset?.maxContext;
      if (maxCtx && maxCtx > 0 && maxCtx !== -1000) {
        const maxChars = maxCtx * 4;
        let total = (systemPrompt?.length || 0) + (postChatText?.length || 0);
        let keepFrom = 0;
        for (let i = msgs.length - 1; i >= 0; i--) {
          total += msgs[i].text?.length || 0;
          if (total > maxChars) { keepFrom = i + 1; break; }
        }
        if (keepFrom > 0) msgs = msgs.slice(keepFrom);
      }

      if (postChatText) {
        msgs = [...msgs, { role: "user", text: postChatText }];
      }

      let result;
      if (provider === "vertex") {
        const c = settings.vertexConfig || {};
        result = await invoke("chat_vertex", {
          messages: msgs,
          system_prompt: systemPrompt,
          model: settings.vertexModel || "gemini-2.5-flash",
          project_id: settings.vertexProjectId || "",
          region: settings.vertexRegion || "us-central1",
          temperature: c.temperature ?? 0.9,
          max_tokens: c.max_tokens ?? 8192,
          top_p: c.top_p ?? null,
          top_k: c.top_k ?? null,
          frequency_penalty: c.frequency_penalty ?? null,
          presence_penalty: c.presence_penalty ?? null,
          thinking_budget: c.thinking_budget ?? null,
          tools_google_search: c.tools_googleSearch ?? false,
          tools_code_execution: c.tools_code_execution ?? false,
        });
      } else if (provider === "gca") {
        const c = settings.gcaConfig || {};
        result = await invoke("chat_gca", {
          messages: msgs,
          system_prompt: systemPrompt,
          model: settings.gcaModel || "gemini-2.5-flash",
          temperature: c.temperature ?? 0.9,
          max_tokens: c.max_tokens ?? 8192,
          top_p: c.top_p ?? null,
          top_k: c.top_k ?? null,
          frequency_penalty: c.frequency_penalty ?? null,
          presence_penalty: c.presence_penalty ?? null,
          thinking_level: c.thinking_level ?? null,
          tools_google_search: c.tools_google_search ?? false,
          tools_google_maps: c.tools_googleMaps ?? false,
          tools_url_context: c.tools_url_context ?? false,
          tools_code_execution: c.tools_code_execution ?? false,
        });
      } else if (provider === "mistral") {
        const c = settings.mistralConfig || {};
        result = await invoke("chat_mistral", {
          messages: msgs,
          system_prompt: systemPrompt,
          model: settings.mistralModel || "mistral-small-2603",
          api_key: settings.mistralKey || "",
          temperature: c.temperature ?? 0.9,
          max_tokens: c.max_tokens ?? 8192,
          top_p: c.top_p ?? null,
          frequency_penalty: c.frequency_penalty ?? null,
          presence_penalty: c.presence_penalty ?? null,
          reasoning_effort: c.reasoning_effort ?? null,
        });
      }

      const responseText = streamingText || result || "";
      if (responseText) {
        const charResult = appendMessage(messages, branches, activeBranchId, "char", responseText, new Date().toLocaleTimeString());
        messages = charResult.messages;
        branches = charResult.branches;

        // HyPA: trigger auto-summarization at unit boundaries
        if (hypaApi?.triggerSummarizationIfNeeded) {
          const h = settings.hypa || {};
          hypaApi.triggerSummarizationIfNeeded(visibleMessages, h.summaryUnit).catch(e => {
            console.error("auto-summarize failed:", e);
            flashError("Auto-summarization failed");
          });
        }
      }
      return responseText;
    } catch (e) {
      const errResult = appendMessage(messages, branches, activeBranchId, "error", `Error: ${e}`, new Date().toLocaleTimeString());
      messages = errResult.messages;
      branches = errResult.branches;
      throw e;
    } finally {
      clearInterval(pollInterval);
      await invoke("stop_streaming").catch(e => console.error("stop_streaming failed:", e));
      streaming = false;
      streamingText = "";
    }
  }

  async function sendMessage() {
    if (!chatInput.trim() || streaming) return;
    const userMsg = chatInput.trim();
    chatInput = "";
    const chatTextarea = document.querySelector('.chat-input');
    if (chatTextarea) chatTextarea.style.height = 'auto';
    // Reload provider label in case user changed settings between sends
    loadProviderLabel();
    try {
      await sendChatMessage(userMsg);
    } catch (e) {
      console.error("sendMessage:", e);
    }
    // Auto-focus input after response completes
    requestAnimationFrame(() => { chatInputEl?.focus(); });
  }

  function autoResize(e) {
    const el = e.target;
    el.style.height = 'auto';
    el.style.height = Math.min(el.scrollHeight, 120) + 'px';
  }

  function handleChatKeydown(e) {
    if (e.key === "Enter" && !e.shiftKey && !isMobile()) {
      e.preventDefault();
      sendMessage();
    }
  }

  function isMobile() {
    return /Android|iPhone|iPad/i.test(navigator.userAgent);
  }

  function clearChat() {
    messages = [];
    branches = [{ id: MAIN_BRANCH_ID, name: "main", headId: null, forkPoint: null }];
    activeBranchId = MAIN_BRANCH_ID;
    forkDropdownId = null;
    // Reset toggles to all-ON for the current preset defs
    for (const d of toggleDefs) activeToggles[d.key] = true;
    activeToggles = { ...activeToggles };
    invoke("cmd_save_session", { session: { messages: [], activeToggles, branches, activeBranchId: MAIN_BRANCH_ID } }).catch(e => {
      console.error("session clear save failed:", e);
      flashError("Failed to clear session on disk");
    });
  }

  function deleteMessage(chatId) {
    // Deleting a message from the flat array. Re-parent children that
    // pointed to this message, and update branch heads if needed.
    const msg = messages.find(m => m.chatId === chatId);
    if (!msg) return;

    const updatedMessages = messages
      .filter(m => m.chatId !== chatId)
      .map(m => m.parentId === chatId ? { ...m, parentId: msg.parentId } : m);

    // If any branch's headId was the deleted message, move head to parent
    const updatedBranches = branches.map(b =>
      b.headId === chatId ? { ...b, headId: msg.parentId } : b
    );

    messages = updatedMessages;
    branches = updatedBranches;
  }

  async function regenerateResponse() {
    if (streaming || visibleMessages.length === 0) return;

    let savedAlts = [];
    let savedUserText = "";

    const lastMsg = visibleMessages[visibleMessages.length - 1];
    if (lastMsg.role === "char") {
      savedAlts = [...(lastMsg.alternatives || []), lastMsg.text];
    }

    // Work on visible messages to find the last user message
    let trimIdx = visibleMessages.length - 1;
    while (trimIdx >= 0 && (visibleMessages[trimIdx].role === "char" || visibleMessages[trimIdx].role === "error")) {
      trimIdx--;
    }
    if (trimIdx < 0) return;
    const lastUser = visibleMessages[trimIdx];
    if (lastUser.role !== "user") return;
    savedUserText = lastUser.text;

    // Remove trailing messages from the flat array by their chatIds
    const toRemove = new Set();
    for (let i = visibleMessages.length - 1; i >= trimIdx; i--) {
      toRemove.add(visibleMessages[i].chatId);
    }
    messages = messages.filter(m => !toRemove.has(m.chatId));

    // Update branch head to the message before the removed ones
    const newHead = trimIdx > 0 ? visibleMessages[trimIdx - 1].chatId : null;
    branches = updateBranchHead(branches, activeBranchId, newHead);

    try {
      await sendChatMessage(savedUserText);
    } catch (e) { console.error("regenerateResponse:", e); }

    // Re-attach alternatives to the new response for swipe
    const updatedVisible = getVisibleMessages(messages, branches, activeBranchId);
    if (savedAlts.length > 0 && updatedVisible.length > 0 && updatedVisible[updatedVisible.length - 1].role === "char") {
      const lastCharId = updatedVisible[updatedVisible.length - 1].chatId;
      messages = messages.map(m =>
        m.chatId === lastCharId ? { ...m, alternatives: savedAlts } : m
      );
    }
  }

  function swipeResponse(msg, direction) {
    if (!msg.alternatives?.length) return;
    if (!Array.isArray(msg._allResponses)) {
      msg._allResponses = [...msg.alternatives, msg.text];
      msg._responseIdx = msg._allResponses.length - 1;
    }
    const total = msg._allResponses.length;
    msg._responseIdx = (msg._responseIdx + direction + total) % total;
    const newText = msg._allResponses[msg._responseIdx];
    const newAlts = msg._allResponses.filter((_, i) => i !== msg._responseIdx);
    // Update in the flat messages array
    messages = messages.map(m =>
      m.chatId === msg.chatId
        ? { ...m, text: newText, alternatives: newAlts, _allResponses: msg._allResponses, _responseIdx: msg._responseIdx }
        : m
    );
  }

  function swipeGreeting(direction) {
    if (greetings.length < 2) return;
    greetingIndex = (greetingIndex + direction + greetings.length) % greetings.length;
    const firstVisible = visibleMessages.length > 0 ? visibleMessages[0] : null;
    if (firstVisible && firstVisible.role === "char") {
      messages = messages.map(m =>
        m.chatId === firstVisible.chatId ? { ...m, text: greetings[greetingIndex] } : m
      );
    }
  }

  async function cancelChat() {
    try {
      await invoke("cancel_chat");
    } catch (e) { console.warn("cancel_chat:", e); }
  }

  function startEdit(msg) {
    editingMsgId = msg.chatId;
    editingText = msg.text;
  }

  function saveEdit(msg) {
    if (editingText.trim() === "") return;
    messages = messages.map(m =>
      m.chatId === msg.chatId ? { ...m, text: editingText } : m
    );
    editingMsgId = null;
    editingText = "";
  }

  function cancelEdit() {
    editingMsgId = null;
    editingText = "";
  }

  /** Basic markdown rendering for char messages.
   *  Handles: ```code blocks```, `inline code`, **bold**, *italic*, line breaks.
   *  Escapes HTML first to prevent XSS, then applies markdown patterns. */
  function renderMarkdown(text) {
    if (!text) return "";
    // Escape HTML entities
    let html = text
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");

    // Fenced code blocks: ```lang\n...\n```
    html = html.replace(/```(\w*)\n([\s\S]*?)```/g, (_match, _lang, code) => {
      return `<pre class="md-code-block"><code>${code.replace(/\n$/, "")}</code></pre>`;
    });
    // Inline code: `code`
    html = html.replace(/`([^`\n]+)`/g, '<code class="md-inline-code">$1</code>');
    // Bold: **text**
    html = html.replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>");
    // Italic: *text* (but not inside **)
    html = html.replace(/(?<!\*)\*([^*\n]+)\*(?!\*)/g, "<em>$1</em>");

    return html;
  }

  /** Regenerate response from a specific char message (not just the last one).
   *  Trims all messages after the target, finds the preceding user message,
   *  and re-sends it. Preserves alternatives for swipe navigation. */
  async function regenerateFrom(targetMsg) {
    if (streaming) return;

    // Find the target in visible messages
    const visIdx = visibleMessages.findIndex(m => m.chatId === targetMsg.chatId);
    if (visIdx < 0) return;

    // Collect existing alternatives from the target message
    let savedAlts = [];
    if (targetMsg.role === "char") {
      savedAlts = [...(targetMsg.alternatives || []), targetMsg.text];
    }

    // Walk backwards from target to find the preceding user message
    let trimIdx = visIdx;
    while (trimIdx >= 0 && (visibleMessages[trimIdx].role === "char" || visibleMessages[trimIdx].role === "error")) {
      trimIdx--;
    }
    if (trimIdx < 0) return;
    const lastUser = visibleMessages[trimIdx];
    if (lastUser.role !== "user") return;

    const savedUserText = lastUser.text;

    // Remove from trimIdx to end of visible chain from the flat array
    const toRemove = new Set();
    for (let i = visibleMessages.length - 1; i >= trimIdx; i--) {
      toRemove.add(visibleMessages[i].chatId);
    }
    messages = messages.filter(m => !toRemove.has(m.chatId));

    // Update branch head
    const newHead = trimIdx > 0 ? visibleMessages[trimIdx - 1].chatId : null;
    branches = updateBranchHead(branches, activeBranchId, newHead);

    try {
      await sendChatMessage(savedUserText);
    } catch (e) { console.error("regenerateFromMsg:", e); }

    // Re-attach alternatives to the new response for swipe
    const updatedVisible = getVisibleMessages(messages, branches, activeBranchId);
    if (savedAlts.length > 0 && updatedVisible.length > 0 && updatedVisible[updatedVisible.length - 1].role === "char") {
      const lastCharId = updatedVisible[updatedVisible.length - 1].chatId;
      messages = messages.map(m =>
        m.chatId === lastCharId ? { ...m, alternatives: savedAlts } : m
      );
    }
  }

  /** Load provider label from settings — called on mount and before display. */
  async function loadProviderLabel() {
    try {
      const settings = await invoke("cmd_load_settings") || {};
      const p = settings.chatProvider || "vertex";
      const labels = { vertex: "Vertex AI", gca: "GCA", mistral: "Mistral" };
      chatProvider = labels[p] || p;
    } catch (e) {
      console.warn("loadProviderLabel:", e);
      chatProvider = "";
    }
  }

  /** Count of alternative responses for a char message (including current). */
  function altCount(msg) {
    if (!msg.alternatives?.length) return 0;
    return (msg._allResponses?.length ?? (msg.alternatives.length + 1));
  }

  // ── Fork actions ────────────────────────────────────────────────────

  /** Fork from a specific message: create a new branch starting from that point.
   *  Like `git checkout -b <name>` from a specific commit. The new branch's
   *  head is the fork point message itself. The user can then type a new message
   *  which diverges from the original conversation. */
  function forkFromMessage(chatId) {
    const branchCount = branches.length;
    const name = `분기 ${branchCount}`;
    const result = createForkBranch(branches, chatId, name);
    branches = result.branches;
    activeBranchId = result.newBranch.id;
    forkDropdownId = null;
  }

  /** Switch to a different branch (git checkout). */
  function switchBranch(branchId) {
    activeBranchId = branchId;
    forkDropdownId = null;
  }

  /** Toggle the fork dropdown for a message. */
  function toggleForkDropdown(chatId) {
    forkDropdownId = forkDropdownId === chatId ? null : chatId;
  }

  /** Close fork dropdown when clicking outside. */
  function closeForkDropdown() {
    forkDropdownId = null;
  }
</script>

<!--
  Layout: messages flow in normal block flow within the parent .content
  scroll context. The chat-input-bar uses position: sticky bottom: 0 so it
  remains visible regardless of message volume — no fragile height
  calculations against viewport units.
-->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="chat-view" onclick={closeForkDropdown}>
  {#if error}
    <div class="chat-error-toast">{error}</div>
  {/if}

  {#if branches.length > 1}
    <div class="branch-bar">
      <svg class="branch-bar-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M6 3v12M18 9a3 3 0 100-6 3 3 0 000 6zM6 21a3 3 0 100-6 3 3 0 000 6zM18 9a9 9 0 01-9 9" />
      </svg>
      <select
        class="branch-select"
        value={activeBranchId}
        onchange={(e) => switchBranch(e.target.value)}
      >
        {#each branches as branch}
          <option value={branch.id}>
            {branch.name}{branch.id === activeBranchId ? " (현재)" : ""}
          </option>
        {/each}
      </select>
      <span class="branch-count">{branches.length}개 브랜치</span>
    </div>
  {/if}

  <div class="chat-messages">
    {#if visibleMessages.length === 0 && !streaming}
      <div class="empty-state">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
        </svg>
        <p>프롬프트를 테스트할 대화를 시작하세요</p>
        <p class="empty-state-hint">1. Settings에서 API 프로바이더 설정</p>
        <p class="empty-state-hint">2. Character 탭에서 캐릭터 로드</p>
        <p class="empty-state-hint">3. Preset 탭에서 프리셋 로드</p>
        <p class="empty-state-hint">4. 아래 입력창에 메시지를 보내세요</p>
      </div>
    {/if}

    {#each visibleMessages as msg, i}
      {@const forkCount = countForks(branches, msg.chatId)}
      {@const branchesHere = getBranchesAtForkPoint(branches, msg.chatId)}
      <div class="message {msg.role}">
        {#if msg.role !== "error"}
          <div class="msg-actions">
            <button
              class="msg-action-btn msg-fork-btn"
              onclick={(e) => { e.stopPropagation(); forkFromMessage(msg.chatId); }}
              disabled={streaming}
              title="포크 (분기 생성)"
              aria-label="이 메시지에서 분기"
            >
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                <path d="M6 3v12M18 9a3 3 0 100-6 3 3 0 000 6zM6 21a3 3 0 100-6 3 3 0 000 6zM18 9a9 9 0 01-9 9" />
              </svg>
            </button>
            {#if msg.role === "char"}
              <button class="msg-action-btn msg-regen-btn" onclick={() => regenerateFrom(msg)} disabled={streaming} title="여기서 재생성" aria-label="이 응답부터 재생성">↻</button>
            {/if}
            <button class="msg-action-btn" onclick={() => startEdit(msg)} disabled={streaming || editingMsgId !== null} title="편집" aria-label="메시지 편집">✏</button>
            <button class="msg-action-btn msg-delete-btn" onclick={() => deleteMessage(msg.chatId)} disabled={streaming} title="삭제" aria-label="메시지 삭제">×</button>
          </div>
        {/if}
        {#if msg.chatId === editingMsgId}
          <div class="edit-area">
            <textarea
              class="edit-textarea"
              bind:value={editingText}
              rows="3"
            ></textarea>
            <div class="edit-buttons">
              <button class="btn btn-sm btn-primary edit-save-btn" onclick={() => saveEdit(msg)}>저장</button>
              <button class="btn btn-sm btn-secondary edit-cancel-btn" onclick={cancelEdit}>취소</button>
            </div>
          </div>
        {:else if msg.role === "error"}
          <div class="message-bubble error-bubble">
            <svg class="error-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="12" cy="12" r="10" /><path d="M12 8v4M12 16h.01" />
            </svg>
            <span>{msg.text}</span>
          </div>
          <div class="error-actions">
            <button class="btn btn-sm btn-danger error-retry-btn" onclick={regenerateResponse} disabled={streaming}>다시 시도</button>
          </div>
        {:else if msg.role === "char"}
          <div class="message-bubble">{@html renderMarkdown(msg.text)}</div>
        {:else}
          <div class="message-bubble">{msg.text}</div>
        {/if}

        {#if msg.role !== "error"}
          <span class="message-time">
            {msg.time}
            {#if msg.role === "char" && altCount(msg) > 0}
              <span class="alt-badge">{altCount(msg)}</span>
            {/if}
          </span>
        {:else}
          <span class="message-time">{msg.time}</span>
        {/if}

        {#if forkCount > 0}
          <div class="fork-indicator" style="position: relative;">
            <button
              class="fork-badge"
              onclick={(e) => { e.stopPropagation(); toggleForkDropdown(msg.chatId); }}
              aria-label="브랜치 목록 보기"
            >
              <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                <path d="M6 3v12M18 9a3 3 0 100-6 3 3 0 000 6zM6 21a3 3 0 100-6 3 3 0 000 6zM18 9a9 9 0 01-9 9" />
              </svg>
              {forkCount + 1}개 분기
            </button>
            {#if forkDropdownId === msg.chatId}
              <div class="fork-dropdown" onclick={(e) => e.stopPropagation()}>
                {#each branchesHere as branch}
                  <button
                    class="fork-dropdown-item"
                    class:active={branch.id === activeBranchId}
                    onclick={() => switchBranch(branch.id)}
                  >
                    <span class="fork-dropdown-name">{branch.name}</span>
                    {#if branch.id === activeBranchId}
                      <span class="fork-dropdown-current">현재</span>
                    {/if}
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        {/if}

        {#if i === 0 && msg.role === "char" && greetings.length >= 2}
          <div class="swipe-nav">
            <button onclick={() => swipeGreeting(-1)} aria-label="이전 인사말">◀</button>
            <span>{greetingIndex + 1}/{greetings.length}</span>
            <button onclick={() => swipeGreeting(1)} aria-label="다음 인사말">▶</button>
          </div>
        {:else if msg.role === "char" && msg.alternatives?.length}
          {@const total = msg._allResponses?.length ?? ((msg.alternatives?.length || 0) + 1)}
          {@const current = (msg._responseIdx ?? (total - 1)) + 1}
          <div class="swipe-nav">
            <button onclick={() => swipeResponse(msg, -1)} disabled={streaming} aria-label="이전 응답">◀</button>
            <span>{current}/{total}</span>
            <button onclick={() => swipeResponse(msg, 1)} disabled={streaming} aria-label="다음 응답">▶</button>
          </div>
        {/if}
      </div>
    {/each}

    {#if streaming}
      <div class="message char">
        <div class="message-bubble">
          {#if streamingText}
            {@html renderMarkdown(streamingText)}
          {:else}
            <div class="spinner" style="margin: 4px auto;"></div>
          {/if}
        </div>
      </div>
    {/if}

    <div bind:this={chatBottom} aria-hidden="true"></div>
  </div>

  {#if toggleDefs.length > 0}
    <div class="toggle-panel-wrapper">
      <button
        class="toggle-panel-trigger"
        onclick={() => { togglePanelOpen = !togglePanelOpen; }}
        aria-expanded={togglePanelOpen}
        aria-label="프롬프트 토글 패널 열기/닫기"
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M12 5v14M5 12h14" />
        </svg>
        토글 ({Object.values(activeToggles).filter(Boolean).length}/{toggleDefs.length})
        <svg
          class="toggle-panel-chevron"
          class:open={togglePanelOpen}
          width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
        >
          <path d="M6 9l6 6 6-6" />
        </svg>
      </button>
      {#if togglePanelOpen}
        <div class="toggle-panel">
          {#each toggleDefs as def (def.key)}
            <label class="toggle-panel-item">
              <span class="toggle-panel-label">{def.label}</span>
              <span class="toggle">
                <input
                  type="checkbox"
                  checked={activeToggles[def.key] !== false}
                  onchange={(e) => { activeToggles[def.key] = e.target.checked; activeToggles = {...activeToggles}; }}
                />
                <span class="toggle-track"></span>
              </span>
            </label>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  <div class="chat-input-bar">
    <div class="input-meta-row">
      {#if chatProvider}
        <span class="provider-badge">{chatProvider}</span>
      {/if}
      {#if branches.length > 1}
        <span class="branch-badge">{branches.find(b => b.id === activeBranchId)?.name || activeBranchId}</span>
      {/if}
      {#if chatInput.length > 0}
        <span class="char-count">{chatInput.length}자</span>
      {/if}
    </div>
    <div class="input-row">
      {#if visibleMessages.length > 0}
        <button class="btn btn-sm btn-secondary" onclick={clearChat} disabled={streaming} style="flex-shrink: 0; padding: 6px 10px; font-size: 11px; align-self: center;">Clear</button>
      {/if}
      {#if visibleMessages.length > 0 && visibleMessages[visibleMessages.length - 1].role === "char"}
        <button class="btn btn-sm btn-secondary" onclick={regenerateResponse} disabled={streaming} style="flex-shrink: 0; padding: 6px 10px; font-size: 11px; align-self: center;" title="응답 재생성" aria-label="응답 재생성">↻</button>
      {/if}
      <textarea
        class="chat-input"
        rows="1"
        placeholder="메시지를 입력하세요..."
        bind:value={chatInput}
        bind:this={chatInputEl}
        onkeydown={handleChatKeydown}
        oninput={autoResize}
        style="height: auto; min-height: 36px;"
      ></textarea>
      {#if streaming}
        <button class="send-btn cancel" onclick={cancelChat} title="취소">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
            <rect x="6" y="6" width="12" height="12" rx="2" />
          </svg>
        </button>
      {:else}
        <button class="send-btn" onclick={sendMessage} disabled={!chatInput.trim()}>
          <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
            <path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z" />
          </svg>
        </button>
      {/if}
    </div>
  </div>
</div>

<style>
  .chat-messages {
    padding-bottom: 88px;
  }
  .chat-input-bar {
    position: fixed;
    bottom: calc(56px + var(--safe-bottom));
    left: 0;
    right: 0;
    z-index: 50;
    background: var(--bg2);
    border-top: 1px solid var(--bg4);
    display: flex;
    flex-direction: column;
  }
  .input-meta-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 12px 0;
    min-height: 18px;
  }
  .provider-badge {
    font-size: 10px;
    font-weight: 600;
    color: var(--accent);
    background: var(--bg4);
    padding: 1px 6px;
    border-radius: 4px;
    letter-spacing: 0.3px;
  }
  .branch-badge {
    font-size: 10px;
    font-weight: 600;
    color: var(--fg2);
    background: var(--bg3);
    border: 1px solid var(--bg4);
    padding: 1px 6px;
    border-radius: 4px;
  }
  .char-count {
    font-size: 10px;
    color: var(--fg3);
    margin-left: auto;
  }
  .input-row {
    display: flex;
    gap: 8px;
    padding: 6px 12px 12px;
    align-items: flex-end;
  }
  .send-btn.cancel {
    background: var(--error, #e53935);
  }
  .message { position: relative; }
  .msg-actions {
    position: absolute;
    top: 2px;
    right: 2px;
    display: flex;
    gap: 2px;
    opacity: 0;
    transition: opacity 0.15s;
  }
  .message:hover .msg-actions,
  .message:active .msg-actions {
    opacity: 1;
  }
  /* On touch devices always show actions since there's no hover */
  @media (pointer: coarse) {
    .msg-actions { opacity: 1; }
  }
  .msg-action-btn {
    background: none;
    border: none;
    color: var(--fg3);
    cursor: pointer;
    font-size: 14px;
    padding: 2px 5px;
    opacity: 0.5;
  }
  .msg-action-btn:hover { opacity: 1; }
  .msg-fork-btn:hover { color: var(--accent, #5c6bc0); }
  .msg-regen-btn:hover { color: var(--accent); }
  .msg-delete-btn:hover { color: var(--error, #e53935); }
  .msg-action-btn:disabled { cursor: not-allowed; opacity: 0.25; }
  .edit-area {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
  }
  .edit-textarea {
    width: 100%;
    min-height: 60px;
    padding: 8px;
    border: 1px solid var(--bg4);
    border-radius: var(--radius-sm, 4px);
    background: var(--bg1, #1e1e1e);
    color: var(--fg1, #e0e0e0);
    font-size: 13px;
    font-family: inherit;
    resize: vertical;
    box-sizing: border-box;
  }
  .edit-textarea:focus {
    outline: none;
    border-color: var(--accent, #5c6bc0);
  }
  .edit-buttons {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
  }
  .edit-save-btn {
    padding: 4px 12px;
    font-size: 12px;
  }
  .edit-cancel-btn {
    padding: 4px 12px;
    font-size: 12px;
  }
  .swipe-nav {
    display: flex; align-items: center; gap: 8px;
    justify-content: center; margin-top: 4px;
    font-size: 12px; color: var(--fg3);
  }
  .swipe-nav button {
    background: var(--bg3); border: 1px solid var(--bg4);
    border-radius: var(--radius-sm); padding: 2px 8px;
    color: var(--fg2); cursor: pointer; font-size: 12px;
  }
  .chat-error-toast {
    position: fixed;
    top: 8px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 100;
    background: var(--error, #e53935);
    color: #fff;
    padding: 6px 16px;
    border-radius: 6px;
    font-size: 12px;
    max-width: 90vw;
    text-align: center;
    box-shadow: 0 2px 8px rgba(0,0,0,0.3);
  }

  /* --- Branch Bar (top, visible when >1 branch) --- */
  .branch-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    background: var(--bg3, #2a2a2e);
    border-bottom: 1px solid var(--bg4);
    font-size: 12px;
    color: var(--fg3);
  }
  .branch-bar-icon {
    flex-shrink: 0;
    color: var(--accent, #5c6bc0);
  }
  .branch-select {
    background: var(--bg2, #1e1e22);
    color: var(--fg1, #e0e0e0);
    border: 1px solid var(--bg4);
    border-radius: var(--radius-sm, 4px);
    padding: 3px 8px;
    font-size: 12px;
    cursor: pointer;
    max-width: 200px;
  }
  .branch-count {
    color: var(--fg3, #999);
    font-size: 11px;
    margin-left: auto;
  }

  /* --- Fork Indicator & Dropdown --- */
  .fork-indicator {
    display: inline-flex;
    align-items: center;
    margin-top: 4px;
  }
  .fork-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    background: var(--bg3, #2a2a2e);
    border: 1px solid var(--bg4);
    border-radius: 10px;
    padding: 2px 8px;
    font-size: 11px;
    color: var(--fg3, #999);
    cursor: pointer;
    user-select: none;
  }
  .fork-badge:hover {
    color: var(--fg1, #e0e0e0);
    border-color: var(--accent, #5c6bc0);
  }
  .fork-dropdown {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 60;
    background: var(--bg2, #1e1e22);
    border: 1px solid var(--bg4);
    border-radius: var(--radius-sm, 4px);
    box-shadow: 0 4px 12px rgba(0,0,0,0.4);
    min-width: 160px;
    max-width: 280px;
    overflow: hidden;
    margin-top: 4px;
  }
  .fork-dropdown-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    padding: 8px 12px;
    background: none;
    border: none;
    border-bottom: 1px solid var(--bg4);
    color: var(--fg2, #ccc);
    font-size: 12px;
    cursor: pointer;
    text-align: left;
  }
  .fork-dropdown-item:last-child {
    border-bottom: none;
  }
  .fork-dropdown-item:hover {
    background: var(--bg3, #2a2a2e);
  }
  .fork-dropdown-item.active {
    background: var(--bg3, #2a2a2e);
    color: var(--accent, #5c6bc0);
  }
  .fork-dropdown-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fork-dropdown-current {
    font-size: 10px;
    color: var(--accent, #5c6bc0);
    flex-shrink: 0;
  }

  /* --- Preset Toggle Panel --- */
  .toggle-panel-wrapper {
    position: fixed;
    bottom: calc(56px + var(--safe-bottom) + 48px);
    left: 0;
    right: 0;
    z-index: 49;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    pointer-events: none;
  }
  .toggle-panel-trigger {
    pointer-events: auto;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    background: var(--bg3, #2a2a2e);
    border: none;
    border-top: 1px solid var(--bg4);
    color: var(--fg3, #999);
    font-size: 11px;
    cursor: pointer;
    user-select: none;
  }
  .toggle-panel-trigger:hover {
    color: var(--fg1, #e0e0e0);
  }
  .toggle-panel-chevron {
    transition: transform 0.2s;
    margin-left: auto;
  }
  .toggle-panel-chevron.open {
    transform: rotate(180deg);
  }
  .toggle-panel {
    pointer-events: auto;
    background: var(--bg2, #1e1e22);
    border-top: 1px solid var(--bg4);
    padding: 8px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 160px;
    overflow-y: auto;
  }
  .toggle-panel-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    cursor: pointer;
  }
  .toggle-panel-label {
    font-size: 12px;
    color: var(--fg2, #ccc);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* --- Error Messages --- */
  .message.error { align-items: flex-start; }
  .error-bubble {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    background: rgba(248, 113, 113, 0.15);
    border: 1px solid var(--red);
    border-radius: 16px;
    border-bottom-left-radius: 4px;
    max-width: 85%;
    padding: 10px 14px;
    font-size: 13px;
    line-height: 1.5;
    color: var(--red);
    white-space: pre-wrap;
    word-wrap: break-word;
  }
  .error-icon {
    flex-shrink: 0;
    margin-top: 1px;
    color: var(--red);
  }
  .error-actions {
    display: flex;
    gap: 6px;
    margin-top: 6px;
  }
  .error-retry-btn {
    font-size: 11px;
    padding: 4px 10px;
  }

  /* --- Markdown in char messages --- */
  .message.char .message-bubble :global(strong) {
    font-weight: 700;
    color: var(--fg);
  }
  .message.char .message-bubble :global(em) {
    font-style: italic;
  }
  .message.char .message-bubble :global(.md-inline-code) {
    background: var(--bg4);
    padding: 1px 5px;
    border-radius: 4px;
    font-family: 'JetBrains Mono', monospace;
    font-size: 12px;
  }
  .message.char .message-bubble :global(.md-code-block) {
    background: var(--bg);
    border: 1px solid var(--bg4);
    border-radius: var(--radius-sm);
    padding: 10px 12px;
    margin: 6px 0;
    overflow-x: auto;
    font-family: 'JetBrains Mono', monospace;
    font-size: 12px;
    line-height: 1.5;
    white-space: pre;
  }
  .message.char .message-bubble :global(.md-code-block code) {
    background: none;
    padding: 0;
    font-family: inherit;
    font-size: inherit;
  }

  /* --- Alt badge (regeneration count) --- */
  .alt-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 8px;
    background: var(--bg4);
    color: var(--fg2);
    font-size: 10px;
    font-weight: 600;
    margin-left: 4px;
    vertical-align: middle;
  }

  /* --- Empty state hints --- */
  .empty-state-hint {
    font-size: 12px;
    color: var(--fg3);
    line-height: 1.8;
  }
</style>
