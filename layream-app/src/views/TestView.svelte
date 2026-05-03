<script>
  import { invoke, isTauri } from "../lib/tauri.js";
  import { onMount, onDestroy } from "svelte";

  let subTab = $state("chat");

  // --- Chat ---
  let messages = $state([]);
  let chatInput = $state("");
  let streaming = $state(false);
  let streamingText = $state("");
  let chatContainer;
  let unlisten;

  onMount(async () => {
    if (isTauri()) {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        unlisten = await listen("chat-chunk", (event) => {
          streamingText += event.payload;
        });
      } catch (e) { console.warn("Event listen failed:", e); }
    }
    // Load persisted HyPA settings
    try {
      const saved = await invoke("cmd_load_settings");
      if (saved?.hypa) {
        const h = saved.hypa;
        if (h.enabled !== undefined) hypaEnabled = h.enabled;
        if (h.summaryModel !== undefined) hypaSummaryModel = h.summaryModel;
        if (h.summaryTemp !== undefined) hypaSummaryTemp = h.summaryTemp;
        if (h.summaryPrompt !== undefined) hypaSummaryPrompt = h.summaryPrompt;
        if (h.summaryUnit !== undefined) hypaSummaryUnit = h.summaryUnit;
        if (h.embeddingProvider !== undefined) hypaEmbeddingProvider = h.embeddingProvider;
        if (h.embeddingModel !== undefined) hypaEmbeddingModel = h.embeddingModel;
        if (h.similarRatio !== undefined) hypaSimilarRatio = h.similarRatio;
      }
      hypaSettingsLoaded = true;
    } catch (e) {
      console.warn("Failed to load HyPA settings:", e);
      hypaSettingsLoaded = true;
    }
    // Load persisted chat session
    try {
      const savedSession = await invoke("cmd_load_session");
      if (savedSession?.messages?.length) messages = savedSession.messages;
    } catch (e) { console.warn("Failed to load session:", e); }
  });

  onDestroy(() => {
    if (unlisten) unlisten();
    clearTimeout(hypaSettingsSaveTimeout);
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

  async function sendMessage() {
    if (!chatInput.trim() || streaming) return;
    const userMsg = chatInput.trim();
    chatInput = "";
    // Reset textarea height after clearing input
    const chatTextarea = document.querySelector('.chat-input');
    if (chatTextarea) chatTextarea.style.height = 'auto';
    messages = [...messages, { role: "user", text: userMsg, time: new Date().toLocaleTimeString() }];
    streaming = true;
    streamingText = "";

    try {
      const settings = await invoke("cmd_load_settings") || {};
      const provider = settings.chatProvider || "vertex";
      const msgs = messages.filter(m => m.role !== "error").map(m => ({
        role: m.role === "char" ? "model" : m.role,
        text: m.text,
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
      }
    } catch (e) {
      messages = [...messages, { role: "error", text: `Error: ${e}`, time: new Date().toLocaleTimeString() }];
    }
    streaming = false;
    streamingText = "";
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

  // --- Autopilot ---
  let autopilotRunning = $state(false);
  let autopilotTurns = $state(5);
  let autopilotStrategy = $state("continue");
  let autopilotMessages = $state("");
  let autopilotLog = $state([]);

  function toggleAutopilot() {
    autopilotRunning = !autopilotRunning;
    if (autopilotRunning) {
      autopilotLog = [...autopilotLog, { turn: 0, status: "Started", time: new Date().toLocaleTimeString() }];
    } else {
      autopilotLog = [...autopilotLog, { turn: 0, status: "Stopped", time: new Date().toLocaleTimeString() }];
    }
  }

  // --- HyPA ---
  let hypaEnabled = $state(false);
  let hypaSummaryModel = $state("");
  let hypaSummaryTemp = $state(0.3);
  let hypaSummaryPrompt = $state("Summarize the following conversation concisely, preserving key facts and emotional context.");
  let hypaSummaryUnit = $state(10);
  let hypaEmbeddingProvider = $state("vertex");
  let hypaEmbeddingModel = $state("gemini-embedding-2");
  let hypaSimilarRatio = $state(0.7);
  let hypaMemoryCount = $state(0);
  let hypaSummaries = $state([]);
  let hypaSettingsLoaded = $state(false);
  let hypaSettingsSaveTimeout;
  let sessionSaveTimeout;

  function scheduleHypaSettingsSave() {
    clearTimeout(hypaSettingsSaveTimeout);
    hypaSettingsSaveTimeout = setTimeout(async () => {
      try {
        const existing = await invoke("cmd_load_settings") || {};
        existing.hypa = {
          enabled: hypaEnabled,
          summaryModel: hypaSummaryModel,
          summaryTemp: hypaSummaryTemp,
          summaryPrompt: hypaSummaryPrompt,
          summaryUnit: hypaSummaryUnit,
          embeddingProvider: hypaEmbeddingProvider,
          embeddingModel: hypaEmbeddingModel,
          similarRatio: hypaSimilarRatio,
        };
        await invoke("cmd_save_settings", { settings: existing });
      } catch (e) { console.warn("Failed to save HyPA settings:", e); }
    }, 500);
  }

  $effect(() => {
    // Track all HyPA config values
    hypaEnabled; hypaSummaryModel; hypaSummaryTemp; hypaSummaryPrompt;
    hypaSummaryUnit; hypaEmbeddingProvider; hypaEmbeddingModel; hypaSimilarRatio;
    // Only save after initial load to avoid overwriting with defaults
    if (hypaSettingsLoaded) {
      scheduleHypaSettingsSave();
    }
  });

  $effect(() => {
    const msgCount = messages.length;
    if (msgCount > 0) {
      clearTimeout(sessionSaveTimeout);
      sessionSaveTimeout = setTimeout(async () => {
        try {
          await invoke("cmd_save_session", { session: { messages } });
        } catch (e) { console.warn("Session save failed:", e); }
      }, 1000);
    }
  });

  async function loadHypa() {
    try {
      const data = await invoke("cmd_load_hypa");
      hypaSummaries = data?.summaries || [];
      hypaMemoryCount = hypaSummaries.length;
    } catch (e) { console.warn("Failed to load HyPA:", e); }
  }

  async function saveHypa() {
    try {
      await invoke("cmd_save_hypa", { summaries: { summaries: hypaSummaries } });
    } catch (e) { console.warn("Failed to save HyPA:", e); }
  }

  function exportHypa() {
    const blob = new Blob([JSON.stringify({ summaries: hypaSummaries }, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a"); a.href = url; a.download = "hypa-export.json"; a.click();
    URL.revokeObjectURL(url);
  }

  let hypaImportStatus = $state("");

  async function importHypa(file) {
    hypaImportStatus = `importing ${file.name}...`;
    try {
      const text = await new Promise((res, rej) => {
        const reader = new FileReader();
        reader.onload = () => res(reader.result);
        reader.onerror = () => rej(reader.error);
        reader.readAsText(file);
      });
      const data = JSON.parse(text);
      if (data?.summaries) {
        hypaSummaries = data.summaries;
        hypaMemoryCount = hypaSummaries.length;
        await saveHypa();
        hypaImportStatus = `imported ${hypaSummaries.length} summaries`;
      } else {
        hypaImportStatus = `no "summaries" key in JSON`;
      }
    } catch (e) { hypaImportStatus = `import error: ${e}`; }
  }

  function clearHypa() {
    hypaSummaries = [];
    hypaMemoryCount = 0;
    saveHypa();
  }

  // --- Request Logs ---
  let requestLogs = $state([]);
  let selectedLog = $state(null);

  async function loadLogs() {
    try {
      requestLogs = await invoke("get_request_logs") || [];
    } catch (e) { console.warn("Failed to load logs:", e); }
  }

  async function clearLogs() {
    try {
      await invoke("clear_request_logs");
      requestLogs = [];
      selectedLog = null;
    } catch (e) { console.warn("Failed to clear logs:", e); }
  }

  // --- Prompt Preview ---
  let previewText = $state("");
  let previewLoading = $state(false);

  async function loadPreview() {
    previewLoading = true;
    try {
      previewText = await invoke("evaluate_cbs", {
        input: "{{// Prompt preview requires a loaded preset}}",
        char_name: "Character",
        user_name: "User",
      });
    } catch (e) {
      previewText = `Error: ${e}`;
    }
    previewLoading = false;
  }
</script>

<div>
  <!-- Sub-tabs -->
  <div class="tab-bar">
    <button class="tab-btn" class:active={subTab === "chat"} onclick={() => subTab = "chat"}>Chat</button>
    <button class="tab-btn" class:active={subTab === "autopilot"} onclick={() => subTab = "autopilot"}>Autopilot</button>
    <button class="tab-btn" class:active={subTab === "hypa"} onclick={() => { subTab = "hypa"; loadHypa(); }}>HyPA</button>
    <button class="tab-btn" class:active={subTab === "preview"} onclick={() => { subTab = "preview"; loadPreview(); }}>Preview</button>
    <button class="tab-btn" class:active={subTab === "logs"} onclick={() => { subTab = "logs"; loadLogs(); }}>Logs</button>
  </div>

  <!-- Chat Tab -->
  {#if subTab === "chat"}
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
          <button class="btn btn-sm btn-secondary" onclick={clearChat} style="flex-shrink: 0; padding: 6px 10px; font-size: 11px; align-self: center;">Clear</button>
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
  {/if}

  <!-- Autopilot Tab -->
  {#if subTab === "autopilot"}
    <div class="card">
      <div class="card-header">
        <span class="card-title">Autopilot Settings</span>
        <button class="btn btn-sm {autopilotRunning ? 'btn-danger' : 'btn-primary'}" onclick={toggleAutopilot}>
          {autopilotRunning ? "Stop" : "Start"}
        </button>
      </div>
      <div class="card-body">
        <div class="field">
          <label class="label">Turns (1-50)</label>
          <input class="input" type="number" min="1" max="50" bind:value={autopilotTurns} />
        </div>
        <div class="field">
          <label class="label">User Message Strategy</label>
          <select class="select" bind:value={autopilotStrategy}>
            <option value="continue">Continue (empty message)</option>
            <option value="predefined">Predefined messages</option>
            <option value="ai">AI-generated</option>
          </select>
        </div>
        {#if autopilotStrategy === "predefined"}
          <div class="field">
            <label class="label">Messages (one per line)</label>
            <textarea class="textarea" rows="4" bind:value={autopilotMessages} placeholder="Hello&#10;How are you?&#10;Tell me more"></textarea>
          </div>
        {/if}
      </div>
    </div>

    {#if autopilotLog.length > 0}
      <div class="card">
        <div class="card-header"><span class="card-title">Execution Log</span></div>
        <div class="card-body" style="max-height: 300px; overflow-y: auto;">
          {#each autopilotLog as entry}
            <div style="font-size: 12px; padding: 4px 0; border-bottom: 1px solid var(--bg4); color: var(--fg2);">
              <span style="color: var(--fg3);">{entry.time}</span> — {entry.status}
            </div>
          {/each}
        </div>
      </div>
    {/if}
  {/if}

  <!-- HyPA Tab -->
  {#if subTab === "hypa"}
    <div class="card">
      <div class="card-header">
        <span class="card-title">HyPA v3 Memory</span>
        <span style="font-size: 12px; color: var(--fg2);">{hypaMemoryCount} memories</span>
      </div>
      <div class="card-body">
        <div class="toggle-row">
          <span style="font-size: 13px;">Enable HyPA</span>
          <label class="toggle">
            <input type="checkbox" bind:checked={hypaEnabled} />
            <span class="toggle-track"></span>
          </label>
        </div>

        {#if hypaEnabled}
          <div class="field">
            <label class="label">Summary Model</label>
            <input class="input" type="text" bind:value={hypaSummaryModel} placeholder="Uses chat provider model" />
          </div>

          <div class="field">
            <label class="label">Summary Temperature: {hypaSummaryTemp}</label>
            <input type="range" min="0" max="1" step="0.1" bind:value={hypaSummaryTemp} />
          </div>

          <div class="field">
            <label class="label">Summary Prompt</label>
            <textarea class="textarea" rows="3" bind:value={hypaSummaryPrompt}></textarea>
          </div>

          <div class="field">
            <label class="label">Summary Unit (messages)</label>
            <input class="input" type="number" min="2" max="50" bind:value={hypaSummaryUnit} />
          </div>

          <div class="field">
            <label class="label">Embedding Provider</label>
            <select class="select" bind:value={hypaEmbeddingProvider}>
              <option value="vertex">Vertex AI OAuth</option>
              <option value="voyage">Voyage AI</option>
            </select>
          </div>

          <div class="field">
            <label class="label">Embedding Model</label>
            <input class="input" type="text" bind:value={hypaEmbeddingModel} />
          </div>

          <div class="field">
            <label class="label">Similar/Recent Ratio: {hypaSimilarRatio}</label>
            <input type="range" min="0" max="1" step="0.1" bind:value={hypaSimilarRatio} />
          </div>

          <div style="display: flex; gap: 6px; margin-top: 12px;">
            <button class="btn btn-sm btn-secondary" onclick={exportHypa}>Export</button>
            <button class="btn btn-sm btn-secondary" style="position: relative; overflow: hidden;">
              Import
              <input type="file" accept="application/json" style="position: absolute; inset: 0; opacity: 0; cursor: pointer;"
                onchange={async (e) => { const f = e.target.files?.[0]; if (f) await importHypa(f); e.target.value = ""; }}
              />
            </button>
            <button class="btn btn-sm btn-danger" onclick={clearHypa}>Clear All</button>
          </div>
          {#if hypaImportStatus}
            <p style="font-size: 11px; color: var(--orange); margin-top: 8px;">{hypaImportStatus}</p>
          {/if}
        {/if}
      </div>
    </div>
  {/if}

  <!-- Prompt Preview Tab -->
  {#if subTab === "preview"}
    <div class="card">
      <div class="card-header">
        <span class="card-title">Assembled Prompt</span>
        {#if previewText}
          <span style="font-size: 11px; color: var(--fg3);">{previewText.length} chars</span>
        {/if}
      </div>
      <div class="card-body">
        {#if previewLoading}
          <div style="text-align: center; padding: 24px;"><div class="spinner" style="margin: 0 auto;"></div></div>
        {:else if previewText}
          <div class="preview" style="max-height: 500px;">{previewText}</div>
        {:else}
          <div class="empty-state">
            <p>Load a preset and configure prompts to see the assembled result</p>
          </div>
        {/if}
      </div>
    </div>
  {/if}

  <!-- Logs Tab -->
  {#if subTab === "logs"}
    <div class="card">
      <div class="card-header">
        <span class="card-title">Request/Response Logs ({requestLogs.length})</span>
        {#if requestLogs.length > 0}
          <button class="btn btn-sm btn-danger" onclick={clearLogs}>Clear</button>
        {/if}
      </div>
      {#if requestLogs.length > 0}
        <div class="card-body" style="max-height: 400px; overflow-y: auto;">
          {#each requestLogs as log, i}
            <div
              style="padding: 8px 0; border-bottom: 1px solid var(--bg4); cursor: pointer; font-size: 12px;"
              onclick={() => selectedLog = selectedLog === i ? null : i}
            >
              <div style="display: flex; justify-content: space-between;">
                <span style="color: var(--accent);">{log.provider || "?"}</span>
                <span style="color: var(--fg3);">{log.timestamp || ""}</span>
              </div>
              <div style="color: var(--fg2);">{log.endpoint || ""} — {log.status || "?"} ({log.duration_ms || 0}ms)</div>
            </div>
            {#if selectedLog === i}
              <div style="padding: 8px; background: var(--bg); border-radius: var(--radius-sm); margin: 4px 0;">
                <div class="label">Request</div>
                <pre style="font-size: 11px; color: var(--fg2); white-space: pre-wrap; word-break: break-all; max-height: 200px; overflow-y: auto;">{JSON.stringify(log.request_body, null, 2)}</pre>
                <div class="label" style="margin-top: 8px;">Response</div>
                <pre style="font-size: 11px; color: var(--fg2); white-space: pre-wrap; word-break: break-all; max-height: 200px; overflow-y: auto;">{JSON.stringify(log.response_body, null, 2)}</pre>
              </div>
            {/if}
          {/each}
        </div>
      {:else}
        <div class="card-body">
          <div class="empty-state">
            <p>No API logs yet. Logs are recorded when API calls are made.</p>
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>
