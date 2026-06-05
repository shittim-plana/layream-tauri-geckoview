<script>
  import { invoke } from "../lib/tauri.js";
  import { onMount, onDestroy } from "svelte";
  import {
    loadSettings as storeLoadSettings,
    saveSettings as storeSaveSettings,
    getSettings as storeGetSettings,
    getWorkspaceVersion,
  } from "../lib/appStore.svelte.js";
  import { createAutosave } from "../lib/autosave.js";
  import { APP_VERSION } from "../lib/version.js";

  let debugLines = $state([]);
  let debugLog = $derived(debugLines.join("\n"));
  function dbg(msg) { debugLines = [...debugLines.slice(-9), msg]; }

  // --- User Persona ---
  // userName is referenced as {{user}} in CBS templates and used as a fallback
  // when a character card does not specify a persona name.
  let userName = $state("User");

  // --- Provider Assignment ---
  let chatProvider = $state("vertex");
  let summaryProvider = $state("vertex");
  let embeddingProvider = $state("vertex");

  // --- Vertex AI OAuth ---
  let vertexStatus = $state(null);
  let vertexProjectId = $state("");
  let vertexRegion = $state("us-central1");
  let vertexModel = $state("gemini-2.5-flash");
  let vertexEmbeddingModel = $state("gemini-embedding-2");
  let vertexFetchedModels = $state([]);
  let vertexFetching = $state(false);
  let vertexConnecting = $state(false);
  let vertexConfig = $state({
    temperature: 0.9,
    top_p: undefined,
    top_k: undefined,
    max_tokens: 8192,
    frequency_penalty: undefined,
    presence_penalty: undefined,
    thinking_budget: 0,
    tools_googleSearch: false,
    tools_code_execution: false,
  });

  // --- GCA ---
  let gcaStatus = $state(null);
  let gcaModel = $state("gemini-2.5-flash");
  let gcaUserName = $state("");
  let gcaUserEmail = $state("");
  let gcaServiceTier = $state("");
  let gcaOptOut = $state(false);
  let gcaProject = $state("");
  let gcaConfig = $state({
    temperature: 0.9,
    top_p: undefined,
    top_k: undefined,
    max_tokens: 8192,
    frequency_penalty: undefined,
    presence_penalty: undefined,
    use_stream: true,
    thinking_level: "none",
    tools_google_search: false,
    tools_googleMaps: false,
    tools_url_context: false,
    tools_code_execution: false,
    media_resolution: undefined,
  });

  // --- Mistral AI ---
  let mistralKey = $state("");
  let mistralModel = $state("mistral-small-2603");
  let mistralModels = $state([]);
  let mistralFetching = $state(false);
  let mistralConfig = $state({
    temperature: 0.9,
    top_p: undefined,
    max_tokens: 8192,
    frequency_penalty: undefined,
    presence_penalty: undefined,
    reasoning_effort: undefined,
  });

  // --- Voyage AI ---
  let voyageKey = $state("");
  let voyageModel = $state("voyage-4-large");

  // --- Model Lists (from reference sources) ---
  const VERTEX_REGIONS = [
    { value: "global", label: "Global (Preview models)" },
    { value: "us-central1", label: "us-central1 (Iowa)" },
    { value: "us-east4", label: "us-east4 (Virginia)" },
    { value: "us-west1", label: "us-west1 (Oregon)" },
    { value: "europe-west1", label: "europe-west1 (Belgium)" },
    { value: "europe-west4", label: "europe-west4 (Netherlands)" },
    { value: "asia-northeast1", label: "asia-northeast1 (Tokyo)" },
    { value: "asia-southeast1", label: "asia-southeast1 (Singapore)" },
  ];

  const VERTEX_DEFAULT_MODELS = [
    "gemini-3.1-pro-preview",
    "gemini-3.1-flash-lite-preview",
    "gemini-3-pro-preview",
    "gemini-3-flash-preview",
    "gemini-2.5-pro",
    "gemini-2.5-flash",
    "gemini-2.5-flash-lite",
  ];

  const VERTEX_EMBEDDING_MODELS = [
    "gemini-embedding-2",
    "gemini-embedding-001",
  ];

  // Source: risu-gca.js mA array (GCA plugin v0.2.2)
  const GCA_MODELS = [
    "gemini-3.1-pro",
    "gemini-3.1-pro-preview",
    "gemini-3.1-flash-lite-preview",
    "gemini-3-pro-preview",
    "gemini-3-flash-preview",
    "gemini-2.5-pro",
    "gemini-2.5-flash",
    "gemini-2.5-flash-lite",
  ];

  const MISTRAL_DEFAULT_MODELS = [
    "mistral-large-latest",
    "magistral-medium-2509",
    "mistral-medium-2508",
    "mistral-small-2603",
    "codestral-2508",
    "ministral-3-8b-2512",
    "labs-devstral-2512",
  ];

  const THINKING_LEVELS = ["none", "low", "medium", "high"];

  const GCA_TOOLS = [
    { id: "google_search", label: "Google Search" },
    { id: "googleMaps", label: "Google Maps" },
    { id: "url_context", label: "URL Context" },
    { id: "code_execution", label: "Code Execution" },
  ];

  // --- Event listener cleanup handles ---
  let cleanupAuthListener;
  let cleanupGcaListener;

  // --- Actions ---
  async function checkVertexStatus() {
    try {
      vertexStatus = await invoke("vertex_oauth_status");
    } catch (e) { vertexStatus = { connected: false, error: String(e) }; }
  }

  let browserList = $state([]);
  let browserPickerUrl = $state("");
  let showBrowserPicker = $state(false);

  async function openWithBrowserPicker(url) {
    dbg(`openWithBrowserPicker: ${url?.slice(0, 80)}...`);
    try {
      const result = await invoke("list_browsers");
      const browsers = result?.browsers || [];
      dbg(`browsers: ${browsers.length} found`);
      if (browsers.length > 1) {
        browserList = browsers;
        browserPickerUrl = url;
        showBrowserPicker = true;
      } else if (browsers.length === 1) {
        await invoke("open_in_browser", { url, package: browsers[0].package });
      } else {
        await invoke("open_custom_tab", { url });
      }
    } catch (e) {
      dbg(`openWithBrowserPicker failed: ${e}`);
      try { await invoke("open_custom_tab", { url }); } catch (e2) { console.warn("open_custom_tab fallback:", e2); }
    }
  }

  async function pickBrowser(pkg) {
    showBrowserPicker = false;
    dbg(`pickBrowser: ${pkg}`);
    try {
      await invoke("open_in_browser", { url: browserPickerUrl, package: pkg });
    } catch (e) { dbg(`open_in_browser failed: ${e}`); }
  }

  // Redirect URI prefixes for GeckoView OAuth intercept.
  // These must match the schemes registered for deep links in the OAuth console.
  const VERTEX_REDIRECT_PREFIX = "com.googleusercontent.apps.317210024447";
  const GCA_REDIRECT_PREFIX = "com.googleusercontent.apps.681255809395";

  /**
   * Attempt OAuth inside a GeckoView dialog. Returns { code } on success,
   * { error } on user cancel or failure, or null if GeckoView is unavailable.
   */
  async function tryGeckoViewOAuth(authUrl, redirectPrefix) {
    try {
      const result = await invoke("open_geckoview_oauth", {
        url: authUrl,
        redirect_uri_prefix: redirectPrefix,
      });
      return result;
    } catch (e) {
      dbg(`GeckoView OAuth unavailable: ${e}`);
      return null;
    }
  }

  async function startVertexAuth() {
    if (vertexConnecting) return;
    vertexConnecting = true;
    dbg("startVertexAuth: calling invoke...");
    try {
      const url = await invoke("vertex_oauth_start");
      dbg(`got url type=${typeof url}, val=${String(url)?.slice(0, 100)}`);
      if (!url) { dbg("url is falsy!"); return; }

      // Try in-app GeckoView OAuth first
      const geckoResult = await tryGeckoViewOAuth(url, VERTEX_REDIRECT_PREFIX);
      if (geckoResult?.code) {
        dbg("GeckoView OAuth got code, exchanging...");
        const result = await invoke("vertex_oauth_callback", { code: geckoResult.code });
        dbg(`vertex_oauth_callback: ${result}`);
        checkVertexStatus();
        return;
      }
      if (geckoResult?.error && geckoResult.error !== "cancelled") {
        dbg(`GeckoView OAuth error: ${geckoResult.error}`);
      }
      if (geckoResult?.error === "cancelled") {
        dbg("GeckoView OAuth cancelled by user");
        return;
      }

      // Fallback: open in external browser
      dbg("Falling back to external browser...");
      await openWithBrowserPicker(url);
    } catch (e) { dbg(`Vertex auth CATCH: ${e}`); } finally { vertexConnecting = false; }
  }

  // Listen for auth completion events emitted by App.svelte deep link handler.
  // Stores cleanup handles so onDestroy can remove them.
  async function listenAuthEvents() {
    const handler = (e) => {
      dbg(`OAuth complete: ${e.detail}`);
      if (e.detail === "vertex") checkVertexStatus();
      if (e.detail === "gca") { checkGcaStatus(); loadGcaProfile(); }
    };
    window.addEventListener("oauth-complete", handler);
    cleanupAuthListener = () => window.removeEventListener("oauth-complete", handler);

    try {
      const { listen } = await import("@tauri-apps/api/event");
      cleanupGcaListener = await listen("gca-auth-complete", (event) => {
        dbg(`GCA loopback result: ${event.payload}`);
        if (event.payload === "ok") { checkGcaStatus(); loadGcaProfile(); }
      });
    } catch (e) { dbg(`event listen failed: ${e}`); }
  }

  onDestroy(() => {
    cleanupAuthListener?.();
    cleanupGcaListener?.();
  });

  async function disconnectVertex() {
    await invoke("vertex_oauth_disconnect");
    vertexStatus = { connected: false };
  }

  async function fetchVertexModels() {
    vertexFetching = true;
    dbg(`Fetching models for region: ${vertexRegion}...`);
    try {
      const result = await invoke("vertex_list_models", { region: vertexRegion });
      dbg(`Fetch result: ${JSON.stringify(result)?.slice(0, 200)}`);
      if (Array.isArray(result) && result.length) {
        vertexFetchedModels = result;
        dbg(`Fetched ${result.length} models`);
      } else {
        dbg(`No models returned`);
      }
    } catch (e) {
      dbg(`Fetch models failed: ${e}`);
    }
    vertexFetching = false;
  }

  async function checkGcaStatus() {
    try {
      gcaStatus = await invoke("gca_oauth_status");
    } catch (e) { gcaStatus = { connected: false, error: String(e) }; }
  }

  async function startGcaAuth() {
    if (gcaConnecting) return;
    gcaConnecting = true;
    dbg("startGcaAuth...");
    try {
      // Try in-app GeckoView OAuth first (uses deep link redirect URI, no loopback)
      const geckoUrl = await invoke("gca_oauth_url");
      if (geckoUrl) {
        const geckoResult = await tryGeckoViewOAuth(geckoUrl, GCA_REDIRECT_PREFIX);
        if (geckoResult?.code) {
          dbg("GeckoView GCA OAuth got code, exchanging...");
          const result = await invoke("gca_oauth_callback", { code: geckoResult.code });
          dbg(`gca_oauth_callback: ${result}`);
          checkGcaStatus();
          loadGcaProfile();
          return;
        }
        if (geckoResult?.error === "cancelled") {
          dbg("GeckoView GCA OAuth cancelled by user");
          return;
        }
        if (geckoResult?.error) {
          dbg(`GeckoView GCA OAuth error: ${geckoResult.error}`);
        }
      }

      // Fallback: loopback server + external browser
      dbg("Falling back to external browser (loopback)...");
      const url = await invoke("gca_oauth_start");
      dbg(`got url: ${url?.slice(0, 80)}...`);
      if (url) {
        await openWithBrowserPicker(url);
      }
    } catch (e) { dbg(`GCA auth error: ${e}`); } finally { gcaConnecting = false; }
  }

  async function disconnectGca() {
    await invoke("gca_oauth_disconnect");
    gcaStatus = { connected: false };
    gcaUserName = ""; gcaUserEmail = ""; gcaServiceTier = ""; gcaOptOut = false;
  }

  let gcaConnecting = $state(false);
  let gcaProjectLoading = $state(false);

  async function loadGcaProfile() {
    try {
      const [projectResult, optOutResult] = await Promise.allSettled([
        invoke("gca_load_code_assist"),
        invoke("gca_check_opt_out"),
      ]);
      if (projectResult.status === "fulfilled" && projectResult.value) {
        gcaServiceTier = `Project: ${projectResult.value}`;
        gcaProject = projectResult.value;
      }
      if (optOutResult.status === "fulfilled") {
        gcaOptOut = optOutResult.value;
      }
    } catch (e) { console.error("GCA profile load failed:", e); dbg(`GCA profile error: ${e}`); }
  }

  async function loadGcaProject() {
    gcaProjectLoading = true;
    try {
      const projectId = await invoke("cmd_gca_load_project");
      if (projectId) {
        gcaProject = projectId;
        gcaServiceTier = `Project: ${projectId}`;
        scheduleSettingsSave();
        dbg(`GCA project loaded: ${projectId}`);
      }
    } catch (e) {
      console.error("GCA project load failed:", e);
      dbg(`GCA project error: ${e}`);
    }
    gcaProjectLoading = false;
  }

  function exportGcaConfig() {
    const data = { gcaModel, gcaConfig };
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a"); a.href = url; a.download = "gca-config.json"; a.click();
    URL.revokeObjectURL(url);
  }

  async function importGcaConfig(name, data) {
    try {
      const text = new TextDecoder().decode(new Uint8Array(data));
      const parsed = JSON.parse(text);
      if (parsed.gcaModel) gcaModel = parsed.gcaModel;
      if (parsed.gcaConfig) gcaConfig = { ...gcaConfig, ...parsed.gcaConfig };
      scheduleSettingsSave();
    } catch (e) { console.error("GCA config import failed:", e); }
  }

  function exportAllSettings() {
    const data = collectSettings();
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a"); a.href = url; a.download = "layream-settings.json"; a.click();
    URL.revokeObjectURL(url);
  }

  async function importAllSettings(name, data) {
    try {
      const text = new TextDecoder().decode(new Uint8Array(data));
      const parsed = JSON.parse(text);
      applySettings(parsed);
      scheduleSettingsSave();
    } catch (e) { console.error("Settings import failed:", e); }
  }

  async function fetchMistralModels() {
    if (!mistralKey) return;
    mistralFetching = true;
    try {
      // Backend filters to chat-capable models and deduplicates by base name
      const result = await invoke("mistral_list_models", { api_key: mistralKey });
      if (result?.length) {
        mistralModels = result.map(m => m.id);
      }
    } catch (e) { console.error("Failed to fetch Mistral models:", e); dbg(`Mistral fetch error: ${e}`); }
    mistralFetching = false;
  }

  function vertexModelOptions() {
    return vertexFetchedModels.length > 0
      ? vertexFetchedModels.map(m => typeof m === "string" ? m : m.id || m.name).filter(Boolean)
      : VERTEX_DEFAULT_MODELS;
  }

  function mistralModelOptions() {
    return mistralModels.length > 0 ? mistralModels : MISTRAL_DEFAULT_MODELS;
  }

  // --- Persistence ---
  const settingsAutosave = createAutosave(async () => {
    try {
      const existing = storeGetSettings() || await invoke("cmd_load_settings") || {};
      const merged = { ...existing, ...collectSettings() };
      await storeSaveSettings(invoke, merged);
    } catch (e) { console.error("Failed to save settings:", e); dbg(`Settings save error: ${e}`); }
  }, { delayMs: 500 });

  function scheduleSettingsSave() {
    settingsAutosave.schedule();
  }

  function collectSettings() {
    return {
      userName,
      chatProvider, summaryProvider, embeddingProvider,
      vertexProjectId, vertexRegion, vertexModel, vertexEmbeddingModel, vertexConfig,
      gcaModel, gcaConfig, gcaProject,
      mistralKey, mistralModel, mistralConfig,
      voyageKey, voyageModel,
    };
  }

  function applySettings(s) {
    if (!s || typeof s !== "object") return;
    if (typeof s.userName === "string" && s.userName.length > 0) userName = s.userName;
    if (s.chatProvider !== undefined) chatProvider = s.chatProvider;
    if (s.summaryProvider !== undefined) summaryProvider = s.summaryProvider;
    if (s.embeddingProvider !== undefined) embeddingProvider = s.embeddingProvider;
    if (s.vertexProjectId !== undefined) vertexProjectId = s.vertexProjectId;
    if (s.vertexRegion !== undefined) vertexRegion = s.vertexRegion;
    if (s.vertexModel !== undefined) vertexModel = s.vertexModel;
    if (s.vertexEmbeddingModel !== undefined) vertexEmbeddingModel = s.vertexEmbeddingModel;
    if (s.vertexConfig) vertexConfig = { ...vertexConfig, ...s.vertexConfig };
    if (s.gcaModel !== undefined) gcaModel = s.gcaModel;
    if (s.gcaConfig) gcaConfig = { ...gcaConfig, ...s.gcaConfig };
    if (s.gcaProject !== undefined) gcaProject = s.gcaProject;
    if (s.mistralKey !== undefined) mistralKey = s.mistralKey;
    if (s.mistralModel !== undefined) mistralModel = s.mistralModel;
    if (s.mistralConfig) mistralConfig = { ...mistralConfig, ...s.mistralConfig };
    if (s.voyageKey !== undefined) voyageKey = s.voyageKey;
    if (s.voyageModel !== undefined) voyageModel = s.voyageModel;
  }

  onMount(async () => {
    try {
      const saved = await storeLoadSettings(invoke);
      applySettings(saved);
    } catch (e) { console.error("Failed to load settings:", e); dbg(`Settings load error: ${e}`); }
    await Promise.all([checkVertexStatus(), checkGcaStatus()]);
    if (gcaStatus?.connected && !gcaStatus?.expired) {
      loadGcaProfile();
    }
    listenAuthEvents();
  });

  // Re-load settings when workspace switches
  $effect(() => {
    const wsVersion = getWorkspaceVersion();
    if (wsVersion === 0) return;
    (async () => {
      try {
        const saved = await storeLoadSettings(invoke);
        applySettings(saved);
      } catch (e) { console.error("Failed to reload settings after workspace switch:", e); }
    })();
  });

  function statusText(status) {
    if (!status) return "Checking...";
    if (status.error) return `Error: ${status.error}`;
    if (!status.connected) return "Not connected";
    if (status.expired) return "Token expired";
    return "Connected";
  }

  function statusClass(status) {
    if (!status || !status.connected) return "disconnected";
    if (status.expired) return "expired";
    return "connected";
  }
</script>

<div>
  <!-- User Persona -->
  <div class="card">
    <div class="card-header"><span class="card-title">User Persona</span></div>
    <div class="card-body">
      <div class="field">
        <label class="label">Display name (used as &#123;&#123;user&#125;&#125; in CBS)</label>
        <input
          class="input"
          type="text"
          bind:value={userName}
          oninput={scheduleSettingsSave}
          placeholder="User"
        />
      </div>
    </div>
  </div>

  <!-- Provider Assignment -->
  <div class="card">
    <div class="card-header"><span class="card-title">Provider Assignment</span></div>
    <div class="card-body">
      <div class="field">
        <label class="label">Chat / Response</label>
        <select class="select" bind:value={chatProvider} onchange={scheduleSettingsSave}>
          <option value="vertex">Vertex AI OAuth</option>
          <option value="gca">Gemini Code Assist</option>
          <option value="mistral">Mistral AI</option>
        </select>
      </div>
      <div class="field">
        <label class="label">HyPA Summary</label>
        <select class="select" bind:value={summaryProvider} onchange={scheduleSettingsSave}>
          <option value="vertex">Vertex AI OAuth</option>
          <option value="gca">Gemini Code Assist</option>
          <option value="mistral">Mistral AI</option>
        </select>
      </div>
      <div class="field">
        <label class="label">Embedding</label>
        <select class="select" bind:value={embeddingProvider} onchange={scheduleSettingsSave}>
          <option value="vertex">Vertex AI OAuth</option>
          <option value="voyage">Voyage AI</option>
        </select>
      </div>
    </div>
  </div>

  <!-- Vertex AI OAuth -->
  <div class="card">
    <div class="card-header">
      <span class="card-title">Vertex AI OAuth</span>
      <span class="status-dot {statusClass(vertexStatus)}" title={statusText(vertexStatus)} role="img" aria-label="Vertex 연결 상태: {statusText(vertexStatus)}"></span>
      {#if vertexConnecting}<span class="status-dot" style="background: var(--blue);"></span>{/if}
    </div>
    <div class="card-body">
      <p role="status" aria-live="polite" style="font-size: 12px; color: var(--fg2); margin-bottom: 12px;">{statusText(vertexStatus)}</p>

      {#if vertexStatus?.connected && !vertexStatus?.expired}
        <div style="display: flex; gap: 6px; margin-bottom: 12px;">
          <button class="btn btn-sm btn-secondary" onclick={checkVertexStatus}>Refresh Status</button>
          <button class="btn btn-sm btn-danger" onclick={disconnectVertex}>Disconnect</button>
        </div>
      {:else}
        <div class="field">
          <label class="label">GCP Project ID</label>
          <input class="input" type="text" bind:value={vertexProjectId} placeholder="my-gcp-project" onchange={scheduleSettingsSave} />
        </div>
        <div class="field">
          <label class="label">Region</label>
          <select class="select" bind:value={vertexRegion} onchange={scheduleSettingsSave}>
            {#each VERTEX_REGIONS as r}<option value={r.value}>{r.label}</option>{/each}
          </select>
        </div>
        <button class="btn btn-primary btn-block" onclick={startVertexAuth}>Connect</button>
      {/if}

      <div class="field" style="margin-top: 12px;">
        <label class="label">Model</label>
        <div style="display: flex; gap: 6px;">
          <select class="select" style="flex:1;" bind:value={vertexModel} onchange={scheduleSettingsSave}>
            {#each vertexModelOptions() as m}<option value={m}>{m}</option>{/each}
          </select>
          <button class="btn btn-sm btn-secondary" onclick={fetchVertexModels} disabled={vertexFetching}>
            {vertexFetching ? "..." : "Fetch"}
          </button>
        </div>
      </div>

      <div class="field">
        <label class="label">Temperature: {vertexConfig.temperature}</label>
        <input type="range" min="0" max="2" step="0.1" bind:value={vertexConfig.temperature} onchange={scheduleSettingsSave} />
      </div>

      <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 8px;">
        <div class="field">
          <label class="label">Top P</label>
          <input class="input" type="number" min="0" max="1" step="0.01" bind:value={vertexConfig.top_p} placeholder="default" onchange={scheduleSettingsSave} />
        </div>
        <div class="field">
          <label class="label">Top K</label>
          <input class="input" type="number" bind:value={vertexConfig.top_k} placeholder="default" onchange={scheduleSettingsSave} />
        </div>
      </div>

      <div class="field">
        <label class="label">Max Output Tokens</label>
        <input class="input" type="number" bind:value={vertexConfig.max_tokens} onchange={scheduleSettingsSave} />
      </div>

      <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 8px;">
        <div class="field">
          <label class="label">Freq Penalty</label>
          <input class="input" type="number" step="0.1" bind:value={vertexConfig.frequency_penalty} placeholder="0" onchange={scheduleSettingsSave} />
        </div>
        <div class="field">
          <label class="label">Presence Penalty</label>
          <input class="input" type="number" step="0.1" bind:value={vertexConfig.presence_penalty} placeholder="0" onchange={scheduleSettingsSave} />
        </div>
      </div>

      <div class="field">
        <label class="label">Thinking Budget (tokens)</label>
        <input class="input" type="number" bind:value={vertexConfig.thinking_budget} min="-1" placeholder="-1 = auto" onchange={scheduleSettingsSave} />
      </div>

      <div style="margin-top: 8px;">
        <label class="label">Tools</label>
        <div class="toggle-row">
          <span style="font-size: 13px;">Google Search</span>
          <label class="toggle">
            <input type="checkbox" bind:checked={vertexConfig.tools_googleSearch} onchange={scheduleSettingsSave} />
            <span class="toggle-track"></span>
          </label>
        </div>
        <div class="toggle-row">
          <span style="font-size: 13px;">Code Execution</span>
          <label class="toggle">
            <input type="checkbox" bind:checked={vertexConfig.tools_code_execution} onchange={scheduleSettingsSave} />
            <span class="toggle-track"></span>
          </label>
        </div>
      </div>
    </div>
  </div>

  <!-- GCA -->
  <div class="card">
    <div class="card-header">
      <span class="card-title">Gemini Code Assist</span>
      <span class="status-dot {statusClass(gcaStatus)}" title={statusText(gcaStatus)} role="img" aria-label="GCA 연결 상태: {statusText(gcaStatus)}"></span>
      {#if gcaConnecting}<span class="status-dot" style="background: var(--blue);"></span>{/if}
    </div>
    <div class="card-body">
      <p role="status" aria-live="polite" style="font-size: 12px; color: var(--fg2); margin-bottom: 12px;">{statusText(gcaStatus)}</p>

      {#if gcaStatus?.connected && !gcaStatus?.expired}
        {#if gcaUserEmail}
          <div style="font-size: 12px; color: var(--fg2); margin-bottom: 8px; padding: 8px; background: var(--bg3); border-radius: var(--radius-sm);">
            <div>{gcaUserName} ({gcaUserEmail})</div>
            <div>Tier: {gcaServiceTier || "Unknown"} · Opt-out: {gcaOptOut ? "Yes" : "No"}</div>
          </div>
        {/if}
        <div style="display: flex; gap: 6px; flex-wrap: wrap; margin-bottom: 12px;">
          <button class="btn btn-sm btn-secondary" onclick={checkGcaStatus}>Refresh Status</button>
          <button class="btn btn-sm btn-secondary" onclick={loadGcaProject} disabled={gcaProjectLoading}>
            {gcaProjectLoading ? "..." : "프로젝트 가져오기"}
          </button>
          <button class="btn btn-sm btn-danger" onclick={disconnectGca}>Disconnect</button>
        </div>
        {#if gcaProject}
          <div style="font-size: 12px; color: var(--fg2); margin-bottom: 8px; padding: 6px 8px; background: var(--bg3); border-radius: var(--radius-sm);">
            Project: {gcaProject}
          </div>
        {/if}
      {:else}
        <button class="btn btn-primary btn-block" onclick={startGcaAuth}>Connect</button>
      {/if}

      <div class="field" style="margin-top: 12px;">
        <label class="label">Model</label>
        <select class="select" bind:value={gcaModel} onchange={scheduleSettingsSave}>
          {#each GCA_MODELS as m}<option value={m}>{m}</option>{/each}
          {#if gcaModel && !GCA_MODELS.includes(gcaModel)}
            <option value={gcaModel}>{gcaModel} (custom)</option>
          {/if}
        </select>
      </div>
      <div class="field">
        <label class="label">Custom Model ID</label>
        <input class="input" type="text" bind:value={gcaModel} placeholder="e.g. gemini-2.5-pro" onchange={scheduleSettingsSave} />
      </div>

      <div class="field">
        <label class="label">Temperature: {gcaConfig.temperature}</label>
        <input type="range" min="0" max="2" step="0.1" bind:value={gcaConfig.temperature} onchange={scheduleSettingsSave} />
      </div>

      <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 8px;">
        <div class="field">
          <label class="label">Top P</label>
          <input class="input" type="number" min="0" max="1" step="0.01" bind:value={gcaConfig.top_p} placeholder="default" onchange={scheduleSettingsSave} />
        </div>
        <div class="field">
          <label class="label">Top K</label>
          <input class="input" type="number" bind:value={gcaConfig.top_k} placeholder="default" onchange={scheduleSettingsSave} />
        </div>
      </div>

      <div class="field">
        <label class="label">Max Output Tokens</label>
        <input class="input" type="number" bind:value={gcaConfig.max_tokens} onchange={scheduleSettingsSave} />
      </div>

      <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 8px;">
        <div class="field">
          <label class="label">Frequency Penalty</label>
          <input class="input" type="number" step="0.1" bind:value={gcaConfig.frequency_penalty} placeholder="0" onchange={scheduleSettingsSave} />
        </div>
        <div class="field">
          <label class="label">Presence Penalty</label>
          <input class="input" type="number" step="0.1" bind:value={gcaConfig.presence_penalty} placeholder="0" onchange={scheduleSettingsSave} />
        </div>
      </div>

      <div class="toggle-row">
        <span style="font-size: 13px;">Streaming</span>
        <label class="toggle">
          <input type="checkbox" bind:checked={gcaConfig.use_stream} onchange={scheduleSettingsSave} />
          <span class="toggle-track"></span>
        </label>
      </div>

      <div class="field">
        <label class="label">Thinking Level</label>
        <select class="select" bind:value={gcaConfig.thinking_level} onchange={scheduleSettingsSave}>
          {#each THINKING_LEVELS as l}<option value={l}>{l}</option>{/each}
        </select>
      </div>

      <div class="field">
        <label class="label">Media Resolution</label>
        <select class="select" bind:value={gcaConfig.media_resolution} onchange={scheduleSettingsSave}>
          <option value="">Default</option>
          <option value="media_resolution_low">Low</option>
          <option value="media_resolution_medium">Medium</option>
          <option value="media_resolution_high">High</option>
        </select>
      </div>

      <div style="margin-top: 8px;">
        <label class="label">Tools</label>
        {#each GCA_TOOLS as tool}
          <div class="toggle-row">
            <span style="font-size: 13px;">{tool.label}</span>
            <label class="toggle">
              <input type="checkbox" bind:checked={gcaConfig[`tools_${tool.id}`]} onchange={scheduleSettingsSave} />
              <span class="toggle-track"></span>
            </label>
          </div>
        {/each}
      </div>

      <div style="display: flex; gap: 6px; margin-top: 12px; border-top: 1px solid var(--bg4); padding-top: 12px;">
        <button class="btn btn-sm btn-secondary" onclick={exportGcaConfig}>Export GCA Config</button>
        <button class="btn btn-sm btn-secondary" style="position: relative; overflow: hidden;">
          Import GCA Config
          <input type="file" accept="application/json" style="position: absolute; inset: 0; opacity: 0; cursor: pointer;"
            onchange={async (e) => {
              const file = e.target.files?.[0];
              if (file) {
                const buf = await file.arrayBuffer();
                importGcaConfig(file.name, Array.from(new Uint8Array(buf)));
              }
              e.target.value = "";
            }}
          />
        </button>
      </div>
    </div>
  </div>

  <!-- Mistral AI -->
  <div class="card">
    <div class="card-header"><span class="card-title">Mistral AI</span></div>
    <div class="card-body">
      <div class="field">
        <label class="label">API Key</label>
        <input class="input" type="password" bind:value={mistralKey} placeholder="..." onchange={scheduleSettingsSave} />
      </div>
      <div class="field">
        <label class="label">Model</label>
        <div style="display: flex; gap: 6px;">
          <select class="select" style="flex:1;" bind:value={mistralModel} onchange={scheduleSettingsSave}>
            {#each mistralModelOptions() as m}<option value={m}>{m}</option>{/each}
          </select>
          <button class="btn btn-sm btn-secondary" onclick={fetchMistralModels} disabled={!mistralKey || mistralFetching}>
            {mistralFetching ? "..." : "Fetch"}
          </button>
        </div>
      </div>

      <div class="field">
        <label class="label">Temperature: {mistralConfig.temperature}</label>
        <input type="range" min="0" max="2" step="0.1" bind:value={mistralConfig.temperature} onchange={scheduleSettingsSave} />
      </div>

      <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 8px;">
        <div class="field">
          <label class="label">Top P</label>
          <input class="input" type="number" min="0" max="1" step="0.01" bind:value={mistralConfig.top_p} placeholder="default" onchange={scheduleSettingsSave} />
        </div>
        <div class="field">
          <label class="label">Max Tokens</label>
          <input class="input" type="number" bind:value={mistralConfig.max_tokens} onchange={scheduleSettingsSave} />
        </div>
      </div>

      <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 8px;">
        <div class="field">
          <label class="label">Freq Penalty</label>
          <input class="input" type="number" step="0.1" bind:value={mistralConfig.frequency_penalty} placeholder="0" onchange={scheduleSettingsSave} />
        </div>
        <div class="field">
          <label class="label">Presence Penalty</label>
          <input class="input" type="number" step="0.1" bind:value={mistralConfig.presence_penalty} placeholder="0" onchange={scheduleSettingsSave} />
        </div>
      </div>

      <div class="field">
        <label class="label">Reasoning Effort</label>
        <select class="select" bind:value={mistralConfig.reasoning_effort} onchange={scheduleSettingsSave}>
          <option value="">None</option>
          <option value="low">Low</option>
          <option value="medium">Medium</option>
          <option value="high">High</option>
        </select>
      </div>

      <p style="font-size: 11px; color: var(--fg3); margin-top: 4px;">Free experience plan available (opt-out possible)</p>
    </div>
  </div>

  <!-- Embedding -->
  <div class="card">
    <div class="card-header"><span class="card-title">Embedding</span></div>
    <div class="card-body">
      <p style="font-size: 12px; color: var(--fg2); margin-bottom: 12px;">HyPA v3 장기 기억에 사용되는 임베딩 설정</p>

      {#if embeddingProvider === "vertex"}
        <div class="field">
          <label class="label">Vertex Embedding Model</label>
          <select class="select" bind:value={vertexEmbeddingModel} onchange={scheduleSettingsSave}>
            {#each VERTEX_EMBEDDING_MODELS as m}<option value={m}>{m}</option>{/each}
          </select>
        </div>
        <p style="font-size: 11px; color: var(--fg3);">Vertex AI OAuth 연결 필요</p>
      {:else}
        <div class="field">
          <label class="label">Voyage API Key</label>
          <input class="input" type="password" bind:value={voyageKey} placeholder="pa-..." onchange={scheduleSettingsSave} />
        </div>
        <div class="field">
          <label class="label">Voyage Model</label>
          <input class="input" type="text" bind:value={voyageModel} onchange={scheduleSettingsSave} />
        </div>
      {/if}
    </div>
  </div>

  <!-- Backup / Restore -->
  <div class="card">
    <div class="card-header"><span class="card-title">Backup / Restore</span></div>
    <div class="card-body">
      <p style="font-size: 12px; color: var(--fg2); margin-bottom: 12px;">Export or import all settings (provider configs, API keys, provider assignments).</p>
      <div style="display: flex; gap: 6px;">
        <button class="btn btn-sm btn-primary" onclick={exportAllSettings}>Export All Settings</button>
        <button class="btn btn-sm btn-secondary" style="position: relative; overflow: hidden;">
          Import Settings
          <input type="file" accept="application/json" style="position: absolute; inset: 0; opacity: 0; cursor: pointer;"
            onchange={async (e) => {
              const file = e.target.files?.[0];
              if (file) {
                const buf = await file.arrayBuffer();
                importAllSettings(file.name, Array.from(new Uint8Array(buf)));
              }
              e.target.value = "";
            }}
          />
        </button>
      </div>
    </div>
  </div>

  <!-- Debug Log -->
  {#if debugLog}
  <div class="card" style="border-color: var(--orange);">
    <div class="card-header"><span class="card-title" style="color: var(--orange);">Debug</span></div>
    <div class="card-body">
      <p style="font-size: 11px; color: var(--orange); word-break: break-all; white-space: pre-wrap;">{debugLog}</p>
    </div>
  </div>
  {/if}

  <!-- About -->
  <div class="card">
    <div class="card-header"><span class="card-title">About</span></div>
    <div class="card-body">
      <p style="font-size: 13px; color: var(--fg2);">
        Layream v{APP_VERSION}<br />Prompt editor &amp; AI testing studio<br />Powered by Rust + Tauri 2.0
      </p>
    </div>
  </div>

  {#if showBrowserPicker}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      style="position: fixed; inset: 0; background: rgba(0,0,0,0.6); z-index: 200; display: flex; align-items: center; justify-content: center; padding: 24px;"
      onclick={() => showBrowserPicker = false}
      ontouchmove={(e) => e.preventDefault()}
    >
      <div style="background: var(--bg2); border-radius: var(--radius); width: 100%; max-width: 320px; overflow: hidden;" onclick={(e) => e.stopPropagation()}>
        <div style="padding: 14px; border-bottom: 1px solid var(--bg4);">
          <span style="font-size: 14px; font-weight: 600;">브라우저 선택</span>
        </div>
        <div style="max-height: 300px; overflow-y: auto; overscroll-behavior: contain;">
          {#each browserList as b}
            <button
              style="width: 100%; padding: 14px; border: none; background: none; color: var(--fg); font-size: 14px; text-align: left; border-bottom: 1px solid var(--bg4); cursor: pointer;"
              onclick={() => pickBrowser(b.package)}
            >
              {b.label}
            </button>
          {/each}
        </div>
        <button style="width: 100%; padding: 12px; border: none; background: var(--bg3); color: var(--fg3); font-size: 13px; cursor: pointer;" onclick={() => showBrowserPicker = false}>
          취소
        </button>
      </div>
    </div>
  {/if}
</div>
