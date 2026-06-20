<script>
  import { invoke } from "../lib/tauri.js";
  import ChatView from "./ChatView.svelte";
  import AutopilotView from "./AutopilotView.svelte";
  import HypaView from "./HypaView.svelte";
  import { assemblePrompt } from "../lib/assemblePrompt.js";
  import { flashError } from "../lib/flashError.js";

  let subTab = $state("chat");

  // Sub-component public interfaces, registered via onReady callbacks.
  // ChatView: { sendChatMessage, getMessages } — Autopilot drives chat through this.
  // HypaView: { loadHypa } — invoked when HyPA tab opens (preserves lazy-load).
  let chatApi = $state(null);
  let hypaApi = $state(null);

  // --- Request Logs ---
  let requestLogs = $state([]);
  let selectedLog = $state(null);
  let logsLoading = $state(false);
  let logsError = $state("");
  let logPersist = $state(false);

  async function loadLogs() {
    logsLoading = true;
    logsError = "";
    try {
      // Load the persistence toggle alongside the logs so the checkbox reflects
      // the backend's restored setting when the tab opens.
      const [persistResult, logsResult] = await Promise.allSettled([
        invoke("get_log_persistence"),
        invoke("get_request_logs"),
      ]);
      if (persistResult.status === "fulfilled") {
        logPersist = persistResult.value === true;
      }
      if (logsResult.status === "fulfilled") {
        requestLogs = logsResult.value || [];
      } else {
        throw logsResult.reason;
      }
    } catch (e) {
      console.error("Failed to load logs:", e);
      logsError = "로그를 불러오지 못했습니다: " + e;
    } finally {
      logsLoading = false;
    }
  }

  async function toggleLogPersist(enabled) {
    try {
      await invoke("set_log_persistence", { enabled });
      logPersist = enabled;
      // Re-read so the list switches between the in-memory buffer and the
      // persisted file according to the new setting.
      await loadLogs();
    } catch (e) {
      console.error("Failed to set log persistence:", e);
      logsError = "로그 파일 저장 설정을 바꾸지 못했습니다: " + e;
    }
  }

  async function clearLogs() {
    try {
      await invoke("clear_request_logs");
      requestLogs = [];
      selectedLog = null;
    } catch (e) {
      console.error("Failed to clear logs:", e);
      logsError = "로그를 지우지 못했습니다: " + e;
    }
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
      let loadedCharacter = null;
      let settings = {};
      try { loadedCharacter = charResult.status === "fulfilled" ? charResult.value : null; } catch (_) {}
      try { settings = settingsResult.status === "fulfilled" ? (settingsResult.value || {}) : {}; } catch (_) {}

      try {
        const pr = await assemblePrompt(invoke, flashError, { loadedCharacter, settings, activeToggles: {}, conversationText: "", queryEmbedding: null });
        previewText = [pr.systemPrompt || "", pr.postChatText || ""].filter(Boolean).join("\n\n--- postChatText ---\n\n") || "(Empty prompt — check preset and character)";
      } catch (assembleErr) {
        previewText = `Prompt assembly requires a loaded preset and character.\n\nDetail: ${assembleErr}`;
      }
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
            <p>프리셋을 불러와서 프롬프트를 구성하면 조립 결과를 볼 수 있습니다</p>
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
        <div style="display: flex; align-items: center; gap: 12px;">
          <label style="display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--fg2); cursor: pointer;">
            <input
              type="checkbox"
              checked={logPersist}
              onchange={(e) => toggleLogPersist(e.currentTarget.checked)}
            />
            로그 파일 저장
          </label>
          {#if requestLogs.length > 0}
            <button class="btn btn-sm btn-danger" onclick={clearLogs}>지우기</button>
          {/if}
        </div>
      </div>
      {#if logsError}
        <div class="status-row" style="border-color: var(--red); color: var(--red);">{logsError}</div>
      {:else if logsLoading}
        <p class="section-note" style="text-align: center; padding: 24px;">로그를 불러오는 중…</p>
      {:else if requestLogs.length > 0}
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
            <p>API 로그가 아직 없습니다. API 호출 시 로그가 기록됩니다.</p>
          </div>
        </div>
      {/if}
    </div>
  {/if}
</div>
