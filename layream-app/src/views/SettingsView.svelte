<script>
  import { invoke } from "../lib/tauri.js";

  let projectId = $state("");
  let region = $state("us-central1");
  let vertexModel = $state("gemini-2.5-flash");
  let vertexAuthStatus = $state("Not connected");
  let gcaModel = $state("gemini-2.5-flash");
  let gcaAuthStatus = $state("Not connected");
  let gcaUserName = $state("");
  let gcaUserEmail = $state("");
  let gcaServiceTier = $state("");
  let gcaOptOut = $state(false);
  let gcaProjectId = $state("");
  let voyageKey = $state("");
  let mistralKey = $state("");
  let mistralModel = $state("mistral-small-2603");
  let mistralModels = $state([]);
  let fetchingMistral = $state(false);
  let vertexFetchedModels = $state([]);
  let fetchingVertex = $state(false);

  let modelConfig = $state({
    temperature: 0.9,
    top_p: undefined,
    min_p: undefined,
    top_k: undefined,
    seed: undefined,
    max_tokens: 8192,
    frequency_penalty: undefined,
    presence_penalty: undefined,
    repetition_penalty: undefined,
    use_stream: true,
    thinking_mode: "level",
    thinking_level: "none",
    thinking_budget: 0,
    media_resolution: undefined,
    active_tools: [],
  });

  const vertexModels = [
    "gemini-3.1-pro-preview",
    "gemini-3.1-flash-lite-preview",
    "gemini-3.0-flash-preview",
    "gemini-2.5-pro",
    "gemini-2.5-flash",
  ];

  const vertexEmbeddingModels = [
    "gemini-embedding-2",
    "gemini-embedding-001",
  ];

  const gcaModels = [
    "gemini-3.1-pro-preview",
    "gemini-3.1-flash-lite-preview",
    "gemini-3-pro-preview",
    "gemini-3-flash-preview",
    "gemini-2.5-pro",
    "gemini-2.5-flash",
    "gemini-2.5-flash-lite",
  ];

  const defaultMistralModels = [
    "mistral-large-latest",
    "magistral-medium-2509",
    "mistral-medium-2508",
    "mistral-small-2603",
    "codestral-2508",
    "ministral-3-8b-2512",
    "labs-devstral-2512",
  ];

  const regions = [
    "us-central1", "us-east4", "us-west1",
    "europe-west1", "asia-northeast1", "global",
  ];

  const thinkingLevels = ["none", "low", "medium", "high"];

  async function startVertexAuth() {
    try {
      const url = await invoke("vertex_oauth_start");
      if (url) { window.open(url, "_blank"); vertexAuthStatus = "Waiting for callback..."; }
    } catch (e) { vertexAuthStatus = `Error: ${e}`; }
  }

  async function checkVertexAuth() {
    try {
      const result = await invoke("vertex_oauth_status");
      vertexAuthStatus = result?.connected ? (result.expired ? "Token expired" : "Connected") : "Not connected";
    } catch (e) { vertexAuthStatus = `Error: ${e}`; }
  }

  async function startGcaAuth() {
    try {
      const url = await invoke("gca_oauth_start");
      if (url) { window.open(url, "_blank"); gcaAuthStatus = "Waiting for callback..."; }
    } catch (e) { gcaAuthStatus = `Error: ${e}`; }
  }

  async function checkGcaAuth() {
    try {
      const result = await invoke("gca_oauth_status");
      gcaAuthStatus = result?.connected ? (result.expired ? "Token expired" : "Connected") : "Not connected";
    } catch (e) { gcaAuthStatus = `Error: ${e}`; }
  }

  async function fetchVertexModels() {
    fetchingVertex = true;
    try {
      const result = await invoke("vertex_list_models", { accessToken: "", region });
      if (result?.length) vertexFetchedModels = result;
    } catch (e) { console.warn("Failed to fetch Vertex models:", e); }
    fetchingVertex = false;
  }

  function vertexSuggestions() {
    return vertexFetchedModels.length > 0 ? vertexFetchedModels : vertexModels;
  }

  async function fetchMistralModels() {
    if (!mistralKey) return;
    fetchingMistral = true;
    try {
      const result = await invoke("mistral_list_models", { apiKey: mistralKey });
      if (result?.length) mistralModels = result.map((m) => m.id).sort();
    } catch (e) { console.warn("Failed to fetch Mistral models:", e); }
    fetchingMistral = false;
  }

  function mistralSuggestions() {
    return mistralModels.length > 0 ? mistralModels : defaultMistralModels;
  }

  function toggleTool(name) {
    const idx = modelConfig.active_tools.indexOf(name);
    if (idx >= 0) modelConfig.active_tools = modelConfig.active_tools.filter(t => t !== name);
    else modelConfig.active_tools = [...modelConfig.active_tools, name];
  }

  function hasTool(name) {
    return modelConfig.active_tools.includes(name);
  }
</script>

<div>
  <!-- Vertex AI OAuth -->
  <div class="card">
    <h3>Vertex AI OAuth</h3>
    <div class="field">
      <label>Project ID</label>
      <input type="text" bind:value={projectId} placeholder="my-gcp-project" />
    </div>
    <div class="field">
      <label>Region</label>
      <select bind:value={region}>
        {#each regions as r}<option value={r}>{r}</option>{/each}
      </select>
    </div>
    <div class="field">
      <label>Model</label>
      <div class="row">
        <select bind:value={vertexModel} style="flex: 1;">
          {#each vertexSuggestions() as m}<option value={m}>{m}</option>{/each}
        </select>
        <button onclick={fetchVertexModels} disabled={fetchingVertex}>
          {fetchingVertex ? "..." : "Fetch"}
        </button>
      </div>
      <input type="text" bind:value={vertexModel} placeholder="custom model ID" class="small-input" />
    </div>
    <div class="field">
      <label>Embedding Model</label>
      <select bind:value={vertexModel}>
        {#each vertexEmbeddingModels as m}<option value={m}>{m}</option>{/each}
      </select>
    </div>
    <div class="row">
      <button onclick={startVertexAuth}>Connect</button>
      <button onclick={checkVertexAuth}>Check Status</button>
    </div>
    <p class="status">{vertexAuthStatus}</p>
  </div>

  <!-- Gemini Code Assist -->
  <div class="card">
    <h3>Gemini Code Assist</h3>
    {#if gcaUserEmail}
    <div class="info-row">
      <span class="info-label">User</span>
      <span>{gcaUserName} ({gcaUserEmail})</span>
    </div>
    <div class="info-row">
      <span class="info-label">Project</span>
      <span class="mono">{gcaProjectId || "N/A"}</span>
    </div>
    <div class="info-row">
      <span class="info-label">Tier</span>
      <span>{gcaServiceTier || "Unknown"}</span>
    </div>
    <div class="info-row">
      <span class="info-label">Opt-out</span>
      <span>{gcaOptOut ? "Yes" : "No"}</span>
    </div>
    {/if}
    <div class="field">
      <label>Model</label>
      <select bind:value={gcaModel} style="width: 100%;">
        {#each gcaModels as m}<option value={m}>{m}</option>{/each}
      </select>
      <input type="text" bind:value={gcaModel} placeholder="custom model ID" class="small-input" />
    </div>
    <div class="row">
      <button onclick={startGcaAuth}>Connect</button>
      <button onclick={checkGcaAuth}>Check Status</button>
    </div>
    <p class="status">{gcaAuthStatus}</p>
  </div>

  <!-- Model Config (shared) -->
  <div class="card">
    <h3>Model Configuration</h3>

    <div class="toggle-row">
      <label>Streaming</label>
      <button class="toggle" class:on={modelConfig.use_stream} onclick={() => modelConfig.use_stream = !modelConfig.use_stream}>
        <span class="toggle-knob"></span>
      </button>
    </div>

    <div class="field">
      <label>Temperature: {modelConfig.temperature}</label>
      <input type="range" min="0" max="2" step="0.1" bind:value={modelConfig.temperature} />
    </div>

    <div class="field">
      <label>Max Output Tokens</label>
      <input type="number" bind:value={modelConfig.max_tokens} />
    </div>

    <div class="field">
      <label>Top P</label>
      <input type="number" min="0" max="1" step="0.01" bind:value={modelConfig.top_p} placeholder="default" />
    </div>

    <div class="field">
      <label>Min P</label>
      <input type="number" min="0" max="1" step="0.01" bind:value={modelConfig.min_p} placeholder="default" />
    </div>

    <div class="field">
      <label>Top K</label>
      <input type="number" bind:value={modelConfig.top_k} placeholder="default" />
    </div>

    <div class="field">
      <label>Seed</label>
      <input type="number" bind:value={modelConfig.seed} placeholder="random" />
    </div>

    <div class="penalties">
      <div class="field">
        <label>Freq Penalty</label>
        <input type="number" step="0.1" bind:value={modelConfig.frequency_penalty} placeholder="0" />
      </div>
      <div class="field">
        <label>Presence Penalty</label>
        <input type="number" step="0.1" bind:value={modelConfig.presence_penalty} placeholder="0" />
      </div>
      <div class="field">
        <label>Repetition Penalty</label>
        <input type="number" step="0.1" bind:value={modelConfig.repetition_penalty} placeholder="0" />
      </div>
    </div>
  </div>

  <!-- Thinking Config -->
  <div class="card">
    <h3>Thinking Configuration</h3>
    <div class="row" style="margin-bottom: 12px;">
      <button class:active={modelConfig.thinking_mode === "level"} onclick={() => modelConfig.thinking_mode = "level"}>Level</button>
      <button class:active={modelConfig.thinking_mode === "tokens"} onclick={() => modelConfig.thinking_mode = "tokens"}>Tokens</button>
    </div>
    {#if modelConfig.thinking_mode === "level"}
    <div class="field">
      <label>Thinking Level</label>
      <select bind:value={modelConfig.thinking_level}>
        {#each thinkingLevels as l}<option value={l}>{l}</option>{/each}
      </select>
    </div>
    {:else}
    <div class="field">
      <label>Thinking Budget (tokens)</label>
      <input type="number" bind:value={modelConfig.thinking_budget} min="-1" placeholder="-1 = auto" />
    </div>
    {/if}
  </div>

  <!-- Active Tools -->
  <div class="card">
    <h3>Active Tools</h3>
    {#each [
      { id: "google_search", label: "Google Search" },
      { id: "googleMaps", label: "Google Maps" },
      { id: "url_context", label: "URL Context" },
      { id: "code_execution", label: "Code Execution" },
    ] as tool}
    <div class="toggle-row">
      <label>{tool.label}</label>
      <button class="toggle" class:on={hasTool(tool.id)} onclick={() => toggleTool(tool.id)}>
        <span class="toggle-knob"></span>
      </button>
    </div>
    {/each}
  </div>

  <!-- Media Resolution -->
  <div class="card">
    <h3>Media Resolution</h3>
    <select bind:value={modelConfig.media_resolution} style="width: 100%;">
      <option value={undefined}>Default</option>
      <option value="media_resolution_low">Low</option>
      <option value="media_resolution_medium">Medium</option>
      <option value="media_resolution_high">High</option>
    </select>
  </div>

  <!-- Mistral AI -->
  <div class="card">
    <h3>Mistral AI</h3>
    <div class="field">
      <label>API Key</label>
      <input type="password" bind:value={mistralKey} placeholder="..." />
    </div>
    <div class="field">
      <label>Model</label>
      <div class="row">
        <select bind:value={mistralModel} style="flex: 1;">
          {#each mistralSuggestions() as m}<option value={m}>{m}</option>{/each}
        </select>
        <button onclick={fetchMistralModels} disabled={!mistralKey || fetchingMistral}>
          {fetchingMistral ? "..." : "Fetch"}
        </button>
      </div>
      <input type="text" bind:value={mistralModel} placeholder="custom model ID" class="small-input" />
    </div>
    <p style="font-size: 12px; color: var(--text-dim);">Free experiment plan available (opt-out).</p>
  </div>

  <!-- Voyage AI -->
  <div class="card">
    <h3>Voyage AI (Embeddings)</h3>
    <div class="field">
      <label>API Key</label>
      <input type="password" bind:value={voyageKey} placeholder="pa-..." />
    </div>
    <p style="font-size: 12px; color: var(--text-dim);">Used for HyPA v3 long-term memory (voyage-4-large)</p>
  </div>

  <!-- About -->
  <div class="card">
    <h3>About</h3>
    <p style="font-size: 13px; color: var(--text-dim);">
      Layream v0.1.0<br />Prompt editor, AI testing studio<br />Powered by Rust
    </p>
  </div>
</div>

<style>
  h3 { margin-bottom: 12px; }
  .status { margin-top: 8px; font-size: 13px; color: var(--text-dim); }
  .row { display: flex; gap: 8px; }
  .small-input { margin-top: 6px; font-size: 12px; }
  .penalties { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 8px; }
  .info-row {
    display: flex; justify-content: space-between; align-items: center;
    padding: 6px 0; font-size: 13px; border-bottom: 1px solid var(--border);
  }
  .info-label { color: var(--text-dim); font-size: 12px; }
  .mono { font-family: monospace; font-size: 12px; }
  select {
    width: 100%; padding: 8px 10px;
    background: var(--surface); color: var(--text);
    border: 1px solid var(--border); border-radius: var(--radius);
    font-size: 14px; appearance: auto;
  }
  .toggle-row {
    display: flex; justify-content: space-between; align-items: center;
    padding: 8px 0;
  }
  .toggle {
    position: relative; width: 44px; height: 24px;
    background: var(--border); border: none; border-radius: 12px;
    cursor: pointer; transition: background 0.2s; padding: 0;
  }
  .toggle.on { background: var(--accent); }
  .toggle-knob {
    position: absolute; top: 2px; left: 2px;
    width: 20px; height: 20px; background: white;
    border-radius: 50%; transition: transform 0.2s;
  }
  .toggle.on .toggle-knob { transform: translateX(20px); }
  button.active { background: var(--accent); }

  @media (max-width: 480px) {
    .penalties { grid-template-columns: 1fr; }
  }
</style>
