<script>
  import { invoke } from "../lib/tauri.js";
  import { onMount, onDestroy } from "svelte";

  let { onReady } = $props();

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
  let hypaImportStatus = $state("");

  onMount(async () => {
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
    onReady?.({ loadHypa });
  });

  onDestroy(() => {
    clearTimeout(hypaSettingsSaveTimeout);
  });

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
</script>

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
