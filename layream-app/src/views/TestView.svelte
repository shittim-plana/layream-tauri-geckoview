<script>
  import { invoke } from "../lib/tauri.js";
  import ChatView from "./ChatView.svelte";
  import AutopilotView from "./AutopilotView.svelte";
  import HypaView from "./HypaView.svelte";

  let subTab = $state("chat");

  // Sub-component public interfaces, registered via onReady callbacks.
  // ChatView: { sendChatMessage, getMessages } — Autopilot drives chat through this.
  // HypaView: { loadHypa } — invoked when HyPA tab opens (preserves lazy-load).
  let chatApi = $state(null);
  let hypaApi = $state(null);

  // --- Request Logs ---
  let requestLogs = $state([]);
  let selectedLog = $state(null);

  async function loadLogs() {
    try {
      requestLogs = await invoke("get_request_logs") || [];
    } catch (e) { console.error("Failed to load logs:", e); }
  }

  async function clearLogs() {
    try {
      await invoke("clear_request_logs");
      requestLogs = [];
      selectedLog = null;
    } catch (e) { console.error("Failed to clear logs:", e); }
  }

  // --- Prompt Preview ---
  let previewText = $state("");
  let previewLoading = $state(false);

  async function loadPreview() {
    previewLoading = true;
    try {
      let charName = "Character";
      let userName = "User";
      const [charResult, settingsResult] = await Promise.allSettled([
        invoke("cmd_load_current_character"),
        invoke("cmd_load_settings"),
      ]);
      if (charResult.status === "fulfilled") {
        const cardName = charResult.value?.card?.data?.name || charResult.value?.card?.name;
        if (typeof cardName === "string" && cardName.length > 0) charName = cardName;
      }
      if (settingsResult.status === "fulfilled" && typeof settingsResult.value?.userName === "string" && settingsResult.value.userName.length > 0) {
        userName = settingsResult.value.userName;
      }
      previewText = await invoke("evaluate_cbs", {
        input: "{{// Prompt preview requires a loaded preset}}",
        char_name: charName,
        user_name: userName,
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
    <button class="tab-btn" class:active={subTab === "hypa"} onclick={() => { subTab = "hypa"; hypaApi?.loadHypa?.(); }}>HyPA</button>
    <button class="tab-btn" class:active={subTab === "preview"} onclick={() => { subTab = "preview"; loadPreview(); }}>Preview</button>
    <button class="tab-btn" class:active={subTab === "logs"} onclick={() => { subTab = "logs"; loadLogs(); }}>Logs</button>
  </div>

  <!--
    Chat / Autopilot / HyPA are kept always-mounted (style:display) so that:
    - ChatView's `chat-chunk` listener stays active across tab switches.
    - Session save $effect and onDestroy flush keep guarding messages.
    - AutopilotView can call into ChatView regardless of which tab is visible.
    This mirrors App.svelte's top-level always-mount pattern.
  -->
  <div style:display={subTab === "chat" ? "block" : "none"}>
    <ChatView onReady={(api) => chatApi = api} {hypaApi} />
  </div>

  <div style:display={subTab === "autopilot" ? "block" : "none"}>
    <AutopilotView {chatApi} />
  </div>

  <div style:display={subTab === "hypa" ? "block" : "none"}>
    <HypaView onReady={(api) => hypaApi = api} />
  </div>

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
