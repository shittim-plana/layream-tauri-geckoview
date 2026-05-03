<script>
  import { invoke } from "../lib/tauri.js";
  import { onMount } from "svelte";

  let debugLog = $state("");
  function dbg(msg) { debugLog = msg; }

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
  let gcaConfig = $state({
    temperature: 0.9,
    top_p: undefined,
    top_k: undefined,
    max_tokens: 8192,
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
    "gemini-2.5-pro",
    "gemini-2.5-flash",
    "gemini-2.0-flash",
    "gemini-2.0-flash-lite",
    "gemini-1.5-pro",
    "gemini-1.5-flash",
  ];

  const VERTEX_EMBEDDING_MODELS = [
    "gemini-embedding-2",
    "gemini-embedding-001",
  ];

  // from risu-gca.js, gemini-3.1-pro removed (user confirmed non-existent)
  const GCA_MODELS = [
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

  // --- Actions ---
  async function checkVertexStatus() {
    try {
      vertexStatus = await invoke("vertex_oauth_status");
    } catch (e) { vertexStatus = { connected: false, error: String(e) }; }
  }

  async function openExternal(url) {
    dbg(`openExternal: ${url?.slice(0, 80)}...`);
    try {
      const { open } = await import("@tauri-apps/plugin-shell");
      await open(url);
      dbg("opened in external browser");
    } catch (e) {
      dbg(`shell.open failed: ${e} — 외부 브라우저를 열 수 없습니다`);
    }
  }

  async function startVertexAuth() {
    dbg("startVertexAuth: calling invoke...");
    try {
      const url = await invoke("vertex_oauth_start");
      dbg(`got url type=${typeof url}, val=${String(url)?.slice(0, 100)}`);
      if (url) await openExternal(url);
      else dbg("url is falsy!");
    } catch (e) { dbg(`Vertex auth CATCH: ${e}`); }
  }

  async function disconnectVertex() {
    await invoke("vertex_oauth_disconnect");
    vertexStatus = { connected: false };
  }

  async function fetchVertexModels() {
    vertexFetching = true;
    try {
      const result = await invoke("vertex_list_models", { region: vertexRegion });
      if (result?.length) vertexFetchedModels = result;
    } catch (e) { console.warn("Failed to fetch Vertex models:", e); }
    vertexFetching = false;
  }

  async function checkGcaStatus() {
    try {
      gcaStatus = await invoke("gca_oauth_status");
    } catch (e) { gcaStatus = { connected: false, error: String(e) }; }
  }

  async function startGcaAuth() {
    dbg("startGcaAuth...");
    try {
      const url = await invoke("gca_oauth_start");
      dbg(`got url: ${url?.slice(0, 80)}...`);
      if (url) {
        await openExternal(url);
        try {
          const { listen } = await import("@tauri-apps/api/event");
          const unlisten = await listen("gca-auth-complete", (event) => {
            dbg(`GCA auth result: ${event.payload}`);
            if (event.payload === "ok") {
              checkGcaStatus();
              loadGcaProfile();
            }
            unlisten();
          });
        } catch (e) { dbg(`event listen failed: ${e}`); }
      }
    } catch (e) { dbg(`GCA auth error: ${e}`); }
  }

  async function disconnectGca() {
    await invoke("gca_oauth_disconnect");
    gcaStatus = { connected: false };
    gcaUserName = ""; gcaUserEmail = ""; gcaServiceTier = ""; gcaOptOut = false;
  }

  async function loadGcaProfile() {
    try {
      const projectId = await invoke("gca_load_code_assist");
      if (projectId) gcaServiceTier = `Project: ${projectId}`;
      const optOut = await invoke("gca_check_opt_out");
      gcaOptOut = optOut;
    } catch (e) { console.warn("GCA profile load failed:", e); }
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
      const result = await invoke("mistral_list_models", { api_key: mistralKey });
      if (result?.length) mistralModels = result.map(m => m.id).sort();
    } catch (e) { console.warn("Failed to fetch Mistral models:", e); }
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
  let saveTimeout;
  function scheduleSettingsSave() {
    clearTimeout(saveTimeout);
    saveTimeout = setTimeout(async () => {
      try {
        const existing = await invoke("cmd_load_settings") || {};
        const merged = { ...existing, ...collectSettings() };
        await invoke("cmd_save_settings", { settings: merged });
      } catch (e) { console.warn("Failed to save settings:", e); }
    }, 500);
  }

  function collectSettings() {
    return {
      chatProvider, summaryProvider, embeddingProvider,
      vertexProjectId, vertexRegion, vertexModel, vertexEmbeddingModel, vertexConfig,
      gcaModel, gcaConfig,
      mistralKey, mistralModel, mistralConfig,
      voyageKey, voyageModel,
    };
  }

  function applySettings(s) {
    if (!s || typeof s !== "object") return;
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
    if (s.mistralKey !== undefined) mistralKey = s.mistralKey;
    if (s.mistralModel !== undefined) mistralModel = s.mistralModel;
    if (s.mistralConfig) mistralConfig = { ...mistralConfig, ...s.mistralConfig };
    if (s.voyageKey !== undefined) voyageKey = s.voyageKey;
    if (s.voyageModel !== undefined) voyageModel = s.voyageModel;
  }

  onMount(async () => {
    try {
      const saved = await invoke("cmd_load_settings");
      applySettings(saved);
    } catch (e) { console.warn("Failed to load settings:", e); }
    checkVertexStatus();
    await checkGcaStatus();
    if (gcaStatus?.connected && !gcaStatus?.expired) {
      loadGcaProfile();
    }
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
      <span class="status-dot {statusClass(vertexStatus)}"></span>
    </div>
    <div class="card-body">
      <p style="font-size: 12px; color: var(--fg2); margin-bottom: 12px;">{statusText(vertexStatus)}</p>

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
        <label class="label">Embedding Model</label>
        <select class="select" bind:value={vertexEmbeddingModel} onchange={scheduleSettingsSave}>
          {#each VERTEX_EMBEDDING_MODELS as m}<option value={m}>{m}</option>{/each}
        </select>
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
      <span class="status-dot {statusClass(gcaStatus)}"></span>
    </div>
    <div class="card-body">
      <p style="font-size: 12px; color: var(--fg2); margin-bottom: 12px;">{statusText(gcaStatus)}</p>

      {#if gcaStatus?.connected && !gcaStatus?.expired}
        {#if gcaUserEmail}
          <div style="font-size: 12px; color: var(--fg2); margin-bottom: 8px; padding: 8px; background: var(--bg3); border-radius: var(--radius-sm);">
            <div>{gcaUserName} ({gcaUserEmail})</div>
            <div>Tier: {gcaServiceTier || "Unknown"} · Opt-out: {gcaOptOut ? "Yes" : "No"}</div>
          </div>
        {/if}
        <div style="display: flex; gap: 6px; margin-bottom: 12px;">
          <button class="btn btn-sm btn-secondary" onclick={checkGcaStatus}>Refresh Status</button>
          <button class="btn btn-sm btn-danger" onclick={disconnectGca}>Disconnect</button>
        </div>
      {:else}
        <button class="btn btn-primary btn-block" onclick={startGcaAuth}>Connect</button>
      {/if}

      <div class="field" style="margin-top: 12px;">
        <label class="label">Model</label>
        <select class="select" bind:value={gcaModel} onchange={scheduleSettingsSave}>
          {#each GCA_MODELS as m}<option value={m}>{m}</option>{/each}
        </select>
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

  <!-- Voyage AI -->
  <div class="card">
    <div class="card-header"><span class="card-title">Voyage AI (Embeddings)</span></div>
    <div class="card-body">
      <div class="field">
        <label class="label">API Key</label>
        <input class="input" type="password" bind:value={voyageKey} placeholder="pa-..." onchange={scheduleSettingsSave} />
      </div>
      <div class="field">
        <label class="label">Model</label>
        <input class="input" type="text" bind:value={voyageModel} onchange={scheduleSettingsSave} />
      </div>
      <p style="font-size: 11px; color: var(--fg3); margin-top: 4px;">Used for HyPA v3 long-term memory</p>
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
      <p style="font-size: 11px; color: var(--orange); word-break: break-all;">{debugLog}</p>
    </div>
  </div>
  {/if}

  <!-- About -->
  <div class="card">
    <div class="card-header"><span class="card-title">About</span></div>
    <div class="card-body">
      <p style="font-size: 13px; color: var(--fg2);">
        Layream v0.2.1-alpha<br />Prompt editor &amp; AI testing studio<br />Powered by Rust + Tauri 2.0
      </p>
    </div>
  </div>
</div>
