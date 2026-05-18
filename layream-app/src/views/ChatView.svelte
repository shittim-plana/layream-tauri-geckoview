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
              await invoke("cmd_save_session", { session: { messages } });
            } catch (e) { console.warn("ChatView app-flush save failed:", e); }
          }
        });
      } catch (e) { console.warn("Event listen failed:", e); }
    }
    // Load persisted chat session
    try {
      const savedSession = await invoke("cmd_load_session");
      if (savedSession?.messages?.length && messages.length === 0) {
        messages = savedSession.messages;
      }
    } catch (e) { console.warn("Failed to load session:", e); }
    sessionLoaded = true;

    // Expose interface to parent
    onReady?.({
      sendChatMessage,
      getMessages: () => messages,
    });
  });

  onDestroy(() => {
    if (unlisten) unlisten();
    if (unlistenAppFlush) unlistenAppFlush();
    // Flush pending session save immediately before clearing timeout
    if (sessionLoaded && messages.length > 0) {
      invoke("cmd_save_session", { session: { messages } }).catch(() => {});
    }
    clearTimeout(sessionSaveTimeout);
  });

  $effect(() => {
    messages;
    streamingText;
    if (chatBottom) {
      requestAnimationFrame(() => {
        chatBottom.scrollIntoView({ block: "end", behavior: "auto" });
      });
    }
  });

  $effect(() => {
    const msgCount = messages.length;
    if (sessionLoaded && msgCount > 0) {
      clearTimeout(sessionSaveTimeout);
      sessionSaveTimeout = setTimeout(async () => {
        try {
          await invoke("cmd_save_session", { session: { messages } });
        } catch (e) { console.warn("Session save failed:", e); }
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
      console.warn("embed for RAG failed:", e);
    }
    return null;
  }

  async function sendChatMessage(userMsg) {
    // first_mes auto-insertion: when starting from an empty session, seed
    // the chat history with the character's greeting (role: "char", matching
    // Layream's stored convention — see msgs.map below mapping char→model).
    // Saved sessions are loaded in onMount before any send, so messages.length === 0
    // here implies a genuinely fresh chat, not a race with session restore.
    if (messages.length === 0) {
      try {
        const ch = await invoke("cmd_load_current_character");
        const card = ch?.card?.data || ch?.card || {};
        const firstMes = card.first_mes || "";
        const altGreetings = card.alternate_greetings || card.alternateGreetings || [];
        const allGreetings = [firstMes, ...altGreetings].filter(g => g && g.trim());
        if (allGreetings.length > 0) {
          greetings = allGreetings;
          greetingIndex = 0;
          messages = [...messages, { chatId: newChatId(), role: "char", text: allGreetings[0], time: new Date().toLocaleTimeString() }];
        }
      } catch (_) {}
    }

    messages = [...messages, { chatId: newChatId(), role: "user", text: userMsg, time: new Date().toLocaleTimeString() }];
    streaming = true;
    streamingText = "";

    try {
      await invoke("start_streaming", { text: "AI 응답 수신 중..." }).catch(() => {});

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
          try {
            const ch = await invoke("cmd_load_current_character");
            const card = ch?.card?.data || ch?.card || {};
            charName = card.name || "Character";
            characterDesc = card.description || "";
            characterPersona = card.personality || "";
            const ext = card.extensions?.risuai || {};
            if (ext.additionalData?.lorebook) {
              lorebook = ext.additionalData.lorebook.filter(e => !e.disable);
            } else if (card.character_book?.entries) {
              lorebook = Object.values(card.character_book.entries).filter(e => !e.enabled === false);
            }
          } catch (_) {}

          const regexList = preset.regex || [];
          const toggles = {};
          if (preset.customPromptTemplateToggle) {
            for (const line of preset.customPromptTemplateToggle.split("\n")) {
              const m = line.match(/^(\w+)\s*[:=]\s*(.+)$/);
              if (m) toggles[`toggle_${m[1]}`] = m[2].trim();
            }
          }

          // Merge module lorebook entries into character lorebook
          try {
            const ch = await invoke("cmd_load_current_character");
            if (ch?.moduleData) {
              const modData = typeof ch.moduleData === "string" ? JSON.parse(ch.moduleData) : ch.moduleData;
              const modLorebook = modData?.lorebook || modData?.data?.lorebook || [];
              const active = (Array.isArray(modLorebook) ? modLorebook : []).filter(e => !e.disable);
              lorebook = [...lorebook, ...active];
            }
          } catch (_) {}

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
                try { text = await invoke("evaluate_cbs", { input: text, char_name: charName, user_name: userName, toggles }); } catch (_) {}
                for (const rx of regexList) {
                  if (rx.type === "editinput" || rx.type === "editoutput") continue;
                  try { text = text.replace(new RegExp(rx.in, rx.flag || "g"), rx.out || ""); } catch (_) {}
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
                } catch (e) { console.warn("memory slot load failed:", e); }
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
      } catch (e) { console.warn("assemblePrompt failed:", e); }

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
        } catch (e) { console.warn("RAG injection failed:", e); }
      }

      let msgs = messages.filter(m => m.role !== "error").map((m, idx, arr) => ({
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
        messages = [...messages, { chatId: newChatId(), role: "char", text: responseText, time: new Date().toLocaleTimeString() }];

        // HyPA: trigger auto-summarization at unit boundaries
        if (hypaApi?.triggerSummarizationIfNeeded) {
          const h = settings.hypa || {};
          hypaApi.triggerSummarizationIfNeeded(messages, h.summaryUnit).catch(e => {
            console.warn("auto-summarize failed:", e);
          });
        }
      }
      return responseText;
    } catch (e) {
      messages = [...messages, { chatId: newChatId(), role: "error", text: `Error: ${e}`, time: new Date().toLocaleTimeString() }];
      throw e;
    } finally {
      await invoke("stop_streaming").catch(() => {});
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
    try {
      await sendChatMessage(userMsg);
    } catch (_) {
      // Error already appended to messages by sendChatMessage
    }
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
    invoke("cmd_save_session", { session: { messages: [] } }).catch(() => {});
  }

  function deleteMessage(chatId) {
    messages = messages.filter(m => m.chatId !== chatId);
  }

  async function regenerateResponse() {
    if (streaming || messages.length === 0) return;

    let savedAlts = [];
    let savedUserText = "";

    const lastMsg = messages[messages.length - 1];
    if (lastMsg.role === "char") {
      savedAlts = [...(lastMsg.alternatives || []), lastMsg.text];
    }

    let trimmed = [...messages];
    while (trimmed.length > 0 && (trimmed[trimmed.length - 1].role === "char" || trimmed[trimmed.length - 1].role === "error")) {
      trimmed.pop();
    }
    if (trimmed.length === 0) return;
    const lastUser = trimmed[trimmed.length - 1];
    if (lastUser.role !== "user") return;
    savedUserText = lastUser.text;
    trimmed.pop();
    messages = trimmed;

    try {
      await sendChatMessage(savedUserText);
    } catch (_) {}

    if (savedAlts.length > 0 && messages.length > 0 && messages[messages.length - 1].role === "char") {
      messages[messages.length - 1].alternatives = savedAlts;
      messages = [...messages];
    }
  }

  function swipeResponse(msg, direction) {
    if (!msg.alternatives?.length) return;
    const all = [...msg.alternatives, msg.text];
    const currentIdx = all.indexOf(msg.text);
    let newIdx = (currentIdx + direction + all.length) % all.length;
    msg.text = all[newIdx];
    msg.alternatives = all.filter(t => t !== msg.text);
    messages = [...messages];
  }

  function swipeGreeting(direction) {
    if (greetings.length < 2) return;
    greetingIndex = (greetingIndex + direction + greetings.length) % greetings.length;
    if (messages.length > 0 && messages[0].role === "char") {
      messages = [{ ...messages[0], text: greetings[greetingIndex] }, ...messages.slice(1)];
    }
  }

  async function cancelChat() {
    try {
      await invoke("cancel_chat");
    } catch (_) {}
  }
</script>

<!--
  Layout: messages flow in normal block flow within the parent .content
  scroll context. The chat-input-bar uses position: sticky bottom: 0 so it
  remains visible regardless of message volume — no fragile height
  calculations against viewport units.
-->
<div class="chat-view">
  <div class="chat-messages">
    {#if messages.length === 0}
      <div class="empty-state">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
        </svg>
        <p>Start a conversation to test your prompts</p>
        <p style="font-size: 12px;">Configure API provider in Settings first</p>
      </div>
    {/if}

    {#each messages as msg, i}
      <div class="message {msg.role}">
        <button class="msg-delete" onclick={() => deleteMessage(msg.chatId)} disabled={streaming} title="삭제" aria-label="메시지 삭제">×</button>
        <div class="message-bubble">{msg.text}</div>
        <span class="message-time">{msg.time}</span>
        {#if i === 0 && msg.role === "char" && greetings.length >= 2}
          <div class="swipe-nav">
            <button onclick={() => swipeGreeting(-1)} aria-label="이전 인사말">◀</button>
            <span>{greetingIndex + 1}/{greetings.length}</span>
            <button onclick={() => swipeGreeting(1)} aria-label="다음 인사말">▶</button>
          </div>
        {:else if msg.role === "char" && msg.alternatives?.length}
          {@const total = msg.alternatives.length + 1}
          {@const current = msg.alternatives.indexOf(msg.text) === -1 ? total : msg.alternatives.indexOf(msg.text) + 1}
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
            {streamingText}
          {:else}
            <div class="spinner" style="margin: 4px auto;"></div>
          {/if}
        </div>
      </div>
    {/if}

    <div bind:this={chatBottom} aria-hidden="true"></div>
  </div>

  <div class="chat-input-bar">
    {#if messages.length > 0}
      <button class="btn btn-sm btn-secondary" onclick={clearChat} disabled={streaming} style="flex-shrink: 0; padding: 6px 10px; font-size: 11px; align-self: center;">Clear</button>
    {/if}
    {#if messages.length > 0 && messages[messages.length - 1].role === "char"}
      <button class="btn btn-sm btn-secondary" onclick={regenerateResponse} disabled={streaming} style="flex-shrink: 0; padding: 6px 10px; font-size: 11px; align-self: center;" title="응답 재생성" aria-label="응답 재생성">↻</button>
    {/if}
    <textarea
      class="chat-input"
      rows="1"
      placeholder="메시지를 입력하세요..."
      bind:value={chatInput}
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

<style>
  .chat-messages {
    padding-bottom: 72px;
  }
  .chat-input-bar {
    position: fixed;
    bottom: calc(56px + var(--safe-bottom));
    left: 0;
    right: 0;
    z-index: 50;
    background: var(--bg2);
    border-top: 1px solid var(--bg4);
  }
  .send-btn.cancel {
    background: var(--error, #e53935);
  }
  .message { position: relative; }
  .msg-delete {
    position: absolute;
    top: 2px;
    right: 2px;
    background: none;
    border: none;
    color: var(--fg3);
    cursor: pointer;
    font-size: 14px;
    padding: 2px 5px;
    opacity: 0.5;
  }
  .msg-delete:hover { opacity: 1; color: var(--error, #e53935); }
  .msg-delete:disabled { cursor: not-allowed; opacity: 0.25; }
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
</style>
