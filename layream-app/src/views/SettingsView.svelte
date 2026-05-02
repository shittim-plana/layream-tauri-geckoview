<script>
  import { invoke } from "../lib/tauri.js";

  let projectId = $state("");
  let region = $state("us-central1");
  let model = $state("gemini-2.5-flash");
  let authStatus = $state("Not connected");
  let voyageKey = $state("");
  let mistralKey = $state("");
  let mistralModel = $state("mistral-small-2603");

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
      <select bind:value={region}>
        {#each regions as r}
          <option value={r}>{r}</option>
        {/each}
      </select>
    </div>
    <div class="field">
      <label>Model</label>
      <select bind:value={model}>
        <optgroup label="Vertex AI">
          {#each vertexModels as m}
            <option value={m}>{m}</option>
          {/each}
        </optgroup>
        <optgroup label="GCA (Gemini Code Assistant)">
          {#each gcaModels as m}
            <option value={`gca:${m}`}>{m}</option>
          {/each}
        </optgroup>
      </select>
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
      <input type="text" bind:value={mistralModel} placeholder="mistral-small-2603" />
    </div>
    <p style="font-size: 12px; color: var(--text-dim);">
      Free experiment plan available (opt-out).
      Use /v1/models to fetch latest model list.
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
