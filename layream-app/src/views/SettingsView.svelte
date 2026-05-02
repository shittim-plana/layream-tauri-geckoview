<script>
  import { invoke } from "../lib/tauri.js";

  let projectId = $state("");
  let region = $state("us-central1");
  let model = $state("gemini-2.5-flash");
  let authStatus = $state("Not connected");
  let voyageKey = $state("");
  let mistralKey = $state("");
  let mistralModel = $state("mistral-small-2603");
  let mistralModels = $state([]);
  let fetchingMistral = $state(false);

  const vertexModels = [
    "gemini-2.5-flash",
    "gemini-2.5-pro",
    "gemini-3.0-flash-preview",
    "gemini-3.0-pro-preview",
    "gemini-3.1-flash-lite-preview",
    "gemini-3.1-pro-preview",
    "gemma-4-31b-it",
    "gemma-4-26b-a4b-it",
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
    <h3 style="margin-bottom: 12px;">Vertex AI OAuth</h3>
    <div class="field">
      <label>Project ID</label>
      <input type="text" bind:value={projectId} placeholder="my-gcp-project" />
    </div>
    <div class="field">
      <label>Region</label>
      <input type="text" list="region-list" bind:value={region} placeholder="us-central1" />
      <datalist id="region-list">
        {#each regions as r}
          <option value={r}></option>
        {/each}
      </datalist>
    </div>
    <div class="field">
      <label>Model</label>
      <input type="text" list="vertex-model-list" bind:value={model} placeholder="gemini-2.5-flash" />
      <datalist id="vertex-model-list">
        <option value="" disabled>— Vertex AI —</option>
        {#each vertexModels as m}
          <option value={m}></option>
        {/each}
        <option value="" disabled>— GCA —</option>
        {#each gcaModels as m}
          <option value={`gca:${m}`}>{m} (GCA)</option>
        {/each}
      </datalist>
      <p style="font-size: 11px; color: var(--text-dim); margin-top: 4px;">
        Select from list or type custom model ID. Prefix with gca: for GCA endpoint.
      </p>
    </div>
    <div style="display: flex; gap: 8px;">
      <button onclick={startAuth}>Connect</button>
      <button onclick={checkAuth}>Check Status</button>
    </div>
    <p style="margin-top: 8px; font-size: 13px; color: var(--text-dim);">{authStatus}</p>
  </div>

  <div class="card">
    <h3 style="margin-bottom: 12px;">Mistral AI</h3>
    <div class="field">
      <label>API Key</label>
      <input type="password" bind:value={mistralKey} placeholder="..." />
    </div>
    <div class="field">
      <label>Model</label>
      <div style="display: flex; gap: 8px;">
        <input
          type="text"
          list="mistral-model-list"
          bind:value={mistralModel}
          placeholder="mistral-small-2603"
          style="flex: 1;"
        />
        <button onclick={fetchMistralModels} disabled={!mistralKey || fetchingMistral} style="white-space: nowrap;">
          {fetchingMistral ? "..." : "Fetch"}
        </button>
      </div>
      <datalist id="mistral-model-list">
        {#each mistralSuggestions() as m}
          <option value={m}></option>
        {/each}
      </datalist>
      <p style="font-size: 11px; color: var(--text-dim); margin-top: 4px;">
        Select from list or type custom model ID. Click Fetch to load latest from API.
      </p>
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
