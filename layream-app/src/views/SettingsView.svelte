<script>
  import { invoke } from "../lib/tauri.js";

  let projectId = $state("");
  let region = $state("us-central1");
  let vertexModel = $state("gemini-2.5-flash");
  let vertexAuthStatus = $state("Not connected");
  let gcaModel = $state("gemini-2.5-flash");
  let gcaAuthStatus = $state("Not connected");
  let voyageKey = $state("");
  let mistralKey = $state("");
  let mistralModel = $state("mistral-small-2603");
  let mistralModels = $state([]);
  let fetchingMistral = $state(false);
  let vertexFetchedModels = $state([]);
  let fetchingVertex = $state(false);

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
    "us-central1",
    "us-east4",
    "us-west1",
    "europe-west1",
    "asia-northeast1",
    "global",
  ];

  async function startVertexAuth() {
    try {
      const url = await invoke("vertex_oauth_start");
      if (url) {
        window.open(url, "_blank");
        vertexAuthStatus = "Waiting for callback...";
      }
    } catch (e) {
      vertexAuthStatus = `Error: ${e}`;
    }
  }

  async function checkVertexAuth() {
    try {
      const result = await invoke("vertex_oauth_status");
      if (result?.connected) {
        vertexAuthStatus = result.expired ? "Token expired — reconnect" : "Connected";
      } else {
        vertexAuthStatus = "Not connected";
      }
    } catch (e) {
      vertexAuthStatus = `Error: ${e}`;
    }
  }

  async function startGcaAuth() {
    try {
      const url = await invoke("gca_oauth_start");
      if (url) {
        window.open(url, "_blank");
        gcaAuthStatus = "Waiting for callback...";
      }
    } catch (e) {
      gcaAuthStatus = `Error: ${e}`;
    }
  }

  async function checkGcaAuth() {
    try {
      const result = await invoke("gca_oauth_status");
      if (result?.connected) {
        gcaAuthStatus = result.expired ? "Token expired — reconnect" : "Connected";
      } else {
        gcaAuthStatus = "Not connected";
      }
    } catch (e) {
      gcaAuthStatus = `Error: ${e}`;
    }
  }

  async function fetchVertexModels() {
    fetchingVertex = true;
    try {
      const result = await invoke("vertex_list_models", { accessToken: "", region });
      if (result?.length) {
        vertexFetchedModels = result;
      }
    } catch (e) {
      console.warn("Failed to fetch Vertex models:", e);
    }
    fetchingVertex = false;
  }

  function vertexSuggestions() {
    if (vertexFetchedModels.length > 0) return vertexFetchedModels;
    return vertexModels;
  }

  async function fetchMistralModels() {
    if (!mistralKey) return;
    fetchingMistral = true;
    try {
      const result = await invoke("mistral_list_models", { apiKey: mistralKey });
      if (result?.length) {
        mistralModels = result.map((m) => m.id).sort();
      }
    } catch (e) {
      console.warn("Failed to fetch Mistral models:", e);
    }
    fetchingMistral = false;
  }

  function mistralSuggestions() {
    if (mistralModels.length > 0) return mistralModels;
    return defaultMistralModels;
  }
</script>

<div>
  <div class="card">
    <h3>Vertex AI OAuth</h3>
    <div class="field">
      <label>Project ID</label>
      <input type="text" bind:value={projectId} placeholder="my-gcp-project" />
    </div>
    <div class="field">
      <label>Region</label>
      <select bind:value={region}>
        {#each regions as r}
          <option value={r}>{r}</option>
        {/each}
      </select>
    </div>
    <div class="field">
      <label>Model</label>
      <div style="display: flex; gap: 8px;">
        <select bind:value={vertexModel} style="flex: 1;">
          {#each vertexSuggestions() as m}
            <option value={m}>{m}</option>
          {/each}
        </select>
        <button onclick={fetchVertexModels} disabled={fetchingVertex}>
          {fetchingVertex ? "..." : "Fetch"}
        </button>
      </div>
      <input type="text" bind:value={vertexModel} placeholder="custom model ID" style="margin-top: 6px; font-size: 12px;" />
    </div>
    <div style="display: flex; gap: 8px;">
      <button onclick={startVertexAuth}>Connect</button>
      <button onclick={checkVertexAuth}>Check Status</button>
    </div>
    <p class="status">{vertexAuthStatus}</p>
  </div>

  <div class="card">
    <h3>Gemini Code Assist</h3>
    <div class="field">
      <label>Model</label>
      <select bind:value={gcaModel} style="width: 100%;">
        {#each gcaModels as m}
          <option value={m}>{m}</option>
        {/each}
      </select>
      <input type="text" bind:value={gcaModel} placeholder="custom model ID" style="margin-top: 6px; font-size: 12px;" />
    </div>
    <div style="display: flex; gap: 8px;">
      <button onclick={startGcaAuth}>Connect</button>
      <button onclick={checkGcaAuth}>Check Status</button>
    </div>
    <p class="status">{gcaAuthStatus}</p>
  </div>

  <div class="card">
    <h3>Mistral AI</h3>
    <div class="field">
      <label>API Key</label>
      <input type="password" bind:value={mistralKey} placeholder="..." />
    </div>
    <div class="field">
      <label>Model</label>
      <div style="display: flex; gap: 8px;">
        <select bind:value={mistralModel} style="flex: 1;">
          {#each mistralSuggestions() as m}
            <option value={m}>{m}</option>
          {/each}
        </select>
        <button onclick={fetchMistralModels} disabled={!mistralKey || fetchingMistral}>
          {fetchingMistral ? "..." : "Fetch"}
        </button>
      </div>
      <input type="text" bind:value={mistralModel} placeholder="custom model ID" style="margin-top: 6px; font-size: 12px;" />
    </div>
    <p style="font-size: 12px; color: var(--text-dim);">
      Free experiment plan available (opt-out).
    </p>
  </div>

  <div class="card">
    <h3>Voyage AI (Embeddings)</h3>
    <div class="field">
      <label>API Key</label>
      <input type="password" bind:value={voyageKey} placeholder="pa-..." />
    </div>
    <p style="font-size: 12px; color: var(--text-dim);">
      Used for HyPA v3 long-term memory (voyage-4-large)
    </p>
  </div>

  <div class="card">
    <h3>About</h3>
    <p style="font-size: 13px; color: var(--text-dim);">
      Layream v0.1.0<br />
      Prompt editor, AI testing studio<br />
      Powered by Rust
    </p>
  </div>
</div>

<style>
  h3 {
    margin-bottom: 12px;
  }
  .status {
    margin-top: 8px;
    font-size: 13px;
    color: var(--text-dim);
  }
  select {
    width: 100%;
    padding: 8px 10px;
    background: var(--bg-input, var(--surface));
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    font-size: 14px;
    appearance: auto;
  }
</style>
