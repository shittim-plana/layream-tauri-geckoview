<script>
  import { invoke } from "../lib/tauri.js";

  let provider = $state("vertex");
  let projectId = $state("");
  let region = $state("us-central1");
  let vertexModel = $state("gemini-2.5-flash");
  let gcaModel = $state("gemini-2.5-flash");
  let authStatus = $state("Not connected");
  let voyageKey = $state("");
  let mistralKey = $state("");
  let mistralModel = $state("mistral-small-2603");
  let mistralModels = $state([]);
  let fetchingMistral = $state(false);
  let vertexFetchedModels = $state([]);
  let fetchingVertex = $state(false);

  const vertexModels = [
    "gemini-2.5-flash",
    "gemini-2.5-pro",
    "gemini-3.0-flash-preview",
    "gemini-3.1-flash-lite-preview",
    "gemini-3.1-pro-preview",
  ];

  const vertexEmbeddingModels = [
    "gemini-embedding-001",
    "gemini-embedding-2",
  ];

  const gcaModels = [
    "gemini-2.5-flash",
    "gemini-2.5-flash-lite",
    "gemini-2.5-pro",
    "gemini-3-flash-preview",
    "gemini-3-pro-preview",
    "gemini-3.1-flash-lite-preview",
    "gemini-3.1-pro",
    "gemini-3.1-pro-preview",
  ];

  const defaultMistralModels = [
    "mistral-small-2603",
    "mistral-medium-2508",
    "magistral-medium-2509",
    "mistral-large-latest",
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

  async function startAuth() {
    try {
      const url = await invoke("oauth_start", { projectId, region });
      if (url) {
        window.open(url, "_blank");
        authStatus = "Waiting for callback...";
      }
    } catch (e) {
      authStatus = `Error: ${e}`;
    }
  }

  async function checkAuth() {
    try {
      const result = await invoke("oauth_status");
      authStatus = result || "Unknown";
    } catch (e) {
      authStatus = `Error: ${e}`;
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
    <h3 style="margin-bottom: 12px;">API Provider</h3>
    <div class="provider-tabs">
      <button class:active={provider === "vertex"} onclick={() => provider = "vertex"}>Vertex AI</button>
      <button class:active={provider === "gca"} onclick={() => provider = "gca"}>GCA</button>
    </div>
  </div>

  {#if provider === "vertex"}
  <div class="card">
    <h3 style="margin-bottom: 12px;">Vertex AI OAuth</h3>
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
        <button onclick={fetchVertexModels} disabled={fetchingVertex} style="white-space: nowrap;">
          {fetchingVertex ? "..." : "Fetch"}
        </button>
      </div>
      <input type="text" bind:value={vertexModel} placeholder="or type custom model ID" style="margin-top: 6px; font-size: 12px;" />
    </div>
    <div style="display: flex; gap: 8px;">
      <button onclick={startAuth}>Connect</button>
      <button onclick={checkAuth}>Check Status</button>
    </div>
    <p style="margin-top: 8px; font-size: 13px; color: var(--text-dim);">{authStatus}</p>
  </div>
  {/if}

  {#if provider === "gca"}
  <div class="card">
    <h3 style="margin-bottom: 12px;">GCA (Gemini Code Assistant)</h3>
    <div class="field">
      <label>Model</label>
      <select bind:value={gcaModel} style="width: 100%;">
        {#each gcaModels as m}
          <option value={m}>{m}</option>
        {/each}
      </select>
      <input type="text" bind:value={gcaModel} placeholder="or type custom model ID" style="margin-top: 6px; font-size: 12px;" />
    </div>
    <div style="display: flex; gap: 8px;">
      <button onclick={startAuth}>Connect</button>
      <button onclick={checkAuth}>Check Status</button>
    </div>
    <p style="margin-top: 8px; font-size: 13px; color: var(--text-dim);">{authStatus}</p>
  </div>
  {/if}

  <div class="card">
    <h3 style="margin-bottom: 12px;">Mistral AI</h3>
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
        <button onclick={fetchMistralModels} disabled={!mistralKey || fetchingMistral} style="white-space: nowrap;">
          {fetchingMistral ? "..." : "Fetch"}
        </button>
      </div>
      <input type="text" bind:value={mistralModel} placeholder="or type custom model ID" style="margin-top: 6px; font-size: 12px;" />
    </div>
    <p style="font-size: 12px; color: var(--text-dim);">
      Free experiment plan available (opt-out).
    </p>
  </div>

  <div class="card">
    <h3 style="margin-bottom: 12px;">Voyage AI (Embeddings)</h3>
    <div class="field">
      <label>API Key</label>
      <input type="password" bind:value={voyageKey} placeholder="pa-..." />
    </div>
    <p style="font-size: 12px; color: var(--text-dim);">
      Used for HyPA v3 long-term memory (voyage-4-large)
    </p>
  </div>

  <div class="card">
    <h3 style="margin-bottom: 8px;">About</h3>
    <p style="font-size: 13px; color: var(--text-dim);">
      Layream v0.1.0<br />
      Prompt editor, AI testing studio<br />
      Powered by Rust
    </p>
  </div>
</div>

<style>
  .provider-tabs {
    display: flex;
    gap: 4px;
  }
  .provider-tabs button {
    flex: 1;
    padding: 8px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text-dim);
    border-radius: 6px;
    cursor: pointer;
  }
  .provider-tabs button.active {
    background: var(--accent);
    color: var(--text);
    border-color: var(--accent);
  }
  select {
    width: 100%;
    padding: 8px 10px;
    background: var(--bg-input);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 6px;
    font-size: 14px;
    appearance: auto;
  }
</style>
