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
  let chatContainer;
  let unlisten;
  let sessionSaveTimeout;

  onMount(async () => {
    if (isTauri()) {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        unlisten = await listen("chat-chunk", (event) => {
          streamingText += event.payload;
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
    // Flush pending session save immediately before clearing timeout
    if (sessionLoaded && messages.length > 0) {
      invoke("cmd_save_session", { session: { messages } }).catch(() => {});
    }
    clearTimeout(sessionSaveTimeout);
  });

  $effect(() => {
    messages;
    streamingText;
    if (chatContainer) {
      requestAnimationFrame(() => {
        chatContainer.scrollTop = chatContainer.scrollHeight;
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
        const v = settings.vertexConfig || {};
        const result = await invoke("embed_vertex", {
          texts: [text],
          model,
          project_id: settings.vertexProjectId || "",
          region: settings.vertexRegion || v.region || "us-central1",
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
    messages = [...messages, { role: "user", text: userMsg, time: new Date().toLocaleTimeString() }];
    streaming = true;
    streamingText = "";

    try {
      const settings = await invoke("cmd_load_settings") || {};
      const provider = settings.chatProvider || "vertex";

      // RAG: retrieve relevant summaries, inject as a memory header into the user message
      let injectedUserMsg = userMsg;
      if (hypaApi?.getRagContext) {
        try {
          const queryEmbedding = await embedQueryForRag(settings, userMsg);
          if (queryEmbedding) {
            const hits = await hypaApi.getRagContext(queryEmbedding, 5);
            if (Array.isArray(hits) && hits.length > 0) {
              const memoryText = hits
                .map(h => h?.summary?.text || h?.text)
                .filter(Boolean)
                .join("\n\n");
              if (memoryText) {
                injectedUserMsg = `[Memory]\n${memoryText}\n\n[User]\n${userMsg}`;
              }
            }
          }
        } catch (e) { console.warn("RAG injection failed:", e); }
      }

      const msgs = messages.filter(m => m.role !== "error").map((m, idx, arr) => ({
        role: m.role === "char" ? "model" : m.role,
        // Replace the last user message text with the RAG-injected version
        text: idx === arr.length - 1 && m.role === "user" ? injectedUserMsg : m.text,
      }));

      let result;
      if (provider === "vertex") {
        const c = settings.vertexConfig || {};
        result = await invoke("chat_vertex", {
          messages: msgs,
          model: settings.vertexModel || "gemini-2.5-flash",
          project_id: settings.vertexProjectId || "",
          region: settings.vertexRegion || "us-central1",
          temperature: c.temperature ?? 0.9,
          max_tokens: c.max_tokens ?? 8192,
          top_p: c.top_p ?? null,
          top_k: c.top_k ?? null,
          thinking_budget: c.thinking_budget ?? null,
          tools_google_search: c.tools_googleSearch ?? false,
          tools_code_execution: c.tools_code_execution ?? false,
        });
      } else if (provider === "gca") {
        const c = settings.gcaConfig || {};
        result = await invoke("chat_gca", {
          messages: msgs,
          model: settings.gcaModel || "gemini-2.5-flash",
          temperature: c.temperature ?? 0.9,
          max_tokens: c.max_tokens ?? 8192,
          top_p: c.top_p ?? null,
          top_k: c.top_k ?? null,
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
        messages = [...messages, { role: "char", text: responseText, time: new Date().toLocaleTimeString() }];

        // HyPA: trigger auto-summarization at unit boundaries
        if (hypaApi?.triggerSummarizationIfNeeded) {
          const h = settings.hypa || {};
          hypaApi.triggerSummarizationIfNeeded(messages, h.summaryUnit ?? 10).catch(e => {
            console.warn("auto-summarize failed:", e);
          });
        }
      }
      return responseText;
    } catch (e) {
      messages = [...messages, { role: "error", text: `Error: ${e}`, time: new Date().toLocaleTimeString() }];
      throw e;
    } finally {
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
</script>

<div style="display: flex; flex-direction: column; height: calc(100dvh - env(safe-area-inset-top, 0px) - 184px - env(safe-area-inset-bottom, 0px));">
  <div bind:this={chatContainer} style="flex: 1; min-height: 0; overflow-y: auto; padding-bottom: 12px;">
    {#if messages.length === 0}
      <div class="empty-state">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
        </svg>
        <p>Start a conversation to test your prompts</p>
        <p style="font-size: 12px;">Configure API provider in Settings first</p>
      </div>
    {/if}

    {#each messages as msg}
      <div class="message {msg.role}">
        <div class="message-bubble">{msg.text}</div>
        <span class="message-time">{msg.time}</span>
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
  </div>

  <div class="chat-input-bar">
    {#if messages.length > 0}
      <button class="btn btn-sm btn-secondary" onclick={clearChat} disabled={streaming} style="flex-shrink: 0; padding: 6px 10px; font-size: 11px; align-self: center;">Clear</button>
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
    <button class="send-btn" onclick={sendMessage} disabled={streaming || !chatInput.trim()}>
      <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
        <path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z" />
      </svg>
    </button>
  </div>
</div>
