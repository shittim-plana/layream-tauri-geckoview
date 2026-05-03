<script>
  import { invoke } from "../lib/tauri.js";
  import { onMount, onDestroy } from "svelte";
  import HypaModal from "../components/HypaModal.svelte";

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
  let hypaActionStatus = $state("");
  let modalSummary = $state(null);
  // last-seen length used by triggerSummarizationIfNeeded() to fire exactly once
  // per crossing of a summaryUnit boundary.
  let lastSummarizedAt = 0;

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
    onReady?.({
      loadHypa,
      // Helper API exported for ChatView wiring (post-merge concern).
      // These wrap the new hypa_* commands which are todo!() stubs at this
      // point — callers must tolerate failure (caught here, logged, returns
      // graceful fallback). Names mirror commands_hypa.rs.
      triggerSummarizationIfNeeded,
      getRagContext,
    });
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

  // ------- Summary list actions -------

  function openModal(index) {
    modalSummary = hypaSummaries[index] ?? null;
  }

  function closeModal() {
    modalSummary = null;
  }

  async function toggleImportantOnModal() {
    if (!modalSummary) return;
    const idx = hypaSummaries.indexOf(modalSummary);
    if (idx < 0) return;
    const next = { ...hypaSummaries[idx], isImportant: !hypaSummaries[idx].isImportant };
    hypaSummaries = [...hypaSummaries.slice(0, idx), next, ...hypaSummaries.slice(idx + 1)];
    modalSummary = next;
    await saveHypa();
  }

  async function deleteSummary(index) {
    if (!confirm("Delete this summary?")) return;
    hypaSummaries = [...hypaSummaries.slice(0, index), ...hypaSummaries.slice(index + 1)];
    hypaMemoryCount = hypaSummaries.length;
    await saveHypa();
  }

  async function refreshHypa() {
    hypaActionStatus = "refreshing...";
    // hypa_load_all is the new (todo!) name; until backend lands, fall back
    // to cmd_load_hypa which has worked since v0.2.0. We try the new name
    // first so once the skeleton is filled in this becomes a no-op switch.
    try {
      const data = await invoke("hypa_load_all");
      hypaSummaries = data?.summaries || [];
      hypaMemoryCount = hypaSummaries.length;
      hypaActionStatus = `refreshed ${hypaSummaries.length} summaries`;
      return;
    } catch (e) {
      console.warn("hypa_load_all failed, falling back to cmd_load_hypa:", e);
    }
    try {
      await loadHypa();
      hypaActionStatus = `refreshed ${hypaSummaries.length} summaries`;
    } catch (e) {
      hypaActionStatus = `refresh failed: ${e}`;
    }
  }

  async function cleanupHypa() {
    hypaActionStatus = "cleaning up...";
    // Try backend cleanup first (returns count of removed entries).
    try {
      const removed = await invoke("hypa_cleanup");
      hypaActionStatus = `removed ${removed ?? 0} empty summaries`;
      // Reload after backend mutation.
      await loadHypa();
      return;
    } catch (e) {
      console.warn("hypa_cleanup failed, doing local cleanup:", e);
    }
    // Fallback: drop locally-empty summaries (chatMemos empty).
    const before = hypaSummaries.length;
    hypaSummaries = hypaSummaries.filter(
      (s) => Array.isArray(s.chatMemos) && s.chatMemos.length > 0
    );
    hypaMemoryCount = hypaSummaries.length;
    await saveHypa();
    hypaActionStatus = `removed ${before - hypaSummaries.length} empty summaries (local)`;
  }

  // ------- Helper API exposed via onReady -------

  // ChatView calls this after each new message. When the message count
  // crosses a multiple of summaryUnit, fire hypa_summarize on the last
  // `summaryUnit` messages. Returns the new summary (object) or null.
  // Caller is responsible for refreshing UI after.
  async function triggerSummarizationIfNeeded(messages, summaryUnit) {
    if (!hypaEnabled) return null;
    const unit = Number(summaryUnit ?? hypaSummaryUnit) || 0;
    if (unit < 2) return null;
    if (!Array.isArray(messages) || messages.length < unit) return null;
    // Fire only when crossing a unit boundary AND we haven't already fired
    // for this length. lastSummarizedAt persists across calls within this
    // component instance.
    if (messages.length % unit !== 0) return null;
    if (messages.length <= lastSummarizedAt) return null;
    lastSummarizedAt = messages.length;
    const slice = messages.slice(messages.length - unit);
    try {
      const result = await invoke("hypa_summarize", {
        messages: slice,
        settings: {
          summaryModel: hypaSummaryModel,
          summaryTemp: hypaSummaryTemp,
          summaryPrompt: hypaSummaryPrompt,
          embeddingProvider: hypaEmbeddingProvider,
          embeddingModel: hypaEmbeddingModel,
        },
      });
      // Backend appends to hypa.json; we reload to stay in sync.
      await loadHypa();
      return result ?? null;
    } catch (e) {
      console.warn("hypa_summarize failed:", e);
      return null;
    }
  }

  // ChatView calls this before sending a chat to retrieve relevant
  // summaries to inject into the prompt context. Returns an array of
  // summary objects (possibly empty). Caller decides how to format them
  // into the prompt.
  async function getRagContext(queryEmbedding, topK = 5) {
    if (!hypaEnabled) return [];
    if (!Array.isArray(queryEmbedding) || queryEmbedding.length === 0) return [];
    try {
      const results = await invoke("hypa_search", {
        queryEmbedding,
        topK,
      });
      return Array.isArray(results) ? results : [];
    } catch (e) {
      console.warn("hypa_search failed:", e);
      return [];
    }
  }

  function summaryPreview(text, max = 120) {
    if (!text) return "(empty)";
    const collapsed = String(text).replace(/\s+/g, " ").trim();
    return collapsed.length <= max ? collapsed : collapsed.slice(0, max) + "…";
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

      <div style="display: flex; gap: 6px; margin-top: 12px; flex-wrap: wrap;">
        <button class="btn btn-sm btn-secondary" onclick={exportHypa}>Export</button>
        <button class="btn btn-sm btn-secondary" style="position: relative; overflow: hidden;">
          Import
          <input type="file" accept="application/json" style="position: absolute; inset: 0; opacity: 0; cursor: pointer;"
            onchange={async (e) => { const f = e.target.files?.[0]; if (f) await importHypa(f); e.target.value = ""; }}
          />
        </button>
        <button class="btn btn-sm btn-secondary" onclick={refreshHypa}>Refresh</button>
        <button class="btn btn-sm btn-secondary" onclick={cleanupHypa}>Cleanup</button>
        <button class="btn btn-sm btn-danger" onclick={clearHypa}>Clear All</button>
      </div>
      {#if hypaImportStatus}
        <p style="font-size: 11px; color: var(--orange); margin-top: 8px;">{hypaImportStatus}</p>
      {/if}
      {#if hypaActionStatus}
        <p style="font-size: 11px; color: var(--fg2); margin-top: 4px;">{hypaActionStatus}</p>
      {/if}
    {/if}
  </div>
</div>

{#if hypaEnabled}
  <div class="card" style="margin-top: 12px;">
    <div class="card-header">
      <span class="card-title">Summaries</span>
      <span style="font-size: 12px; color: var(--fg2);">{hypaSummaries.length}</span>
    </div>
    <div class="card-body">
      {#if hypaSummaries.length === 0}
        <p style="font-size: 12px; color: var(--fg2);">No summaries yet. They'll appear here after auto-summarization or import.</p>
      {:else}
        <div style="display: flex; flex-direction: column; gap: 8px;">
          {#each hypaSummaries as summary, idx (idx)}
            <div style="border: 1px solid var(--bg4, #333); border-radius: 6px; padding: 8px 10px; display: flex; flex-direction: column; gap: 6px;">
              <div style="display: flex; align-items: center; gap: 8px; flex-wrap: wrap;">
                <span style="font-size: 11px; color: var(--fg2);">#{idx + 1}</span>
                <span style="font-size: 11px; color: var(--fg2);">
                  {summary.chatMemos?.length ?? 0} msg{(summary.chatMemos?.length ?? 0) === 1 ? "" : "s"}
                </span>
                {#if summary.isImportant}
                  <span style="font-size: 10px; padding: 1px 6px; border-radius: 999px; background: rgba(234, 179, 8, 0.2); color: #facc15;">important</span>
                {/if}
                {#if (summary.pinBoost ?? 0) > 0}
                  <span style="font-size: 10px; padding: 1px 6px; border-radius: 999px; background: rgba(59, 130, 246, 0.2); color: #93c5fd;">pin {summary.pinBoost}</span>
                {/if}
                {#if summary.invalidated}
                  <span style="font-size: 10px; padding: 1px 6px; border-radius: 999px; background: rgba(239, 68, 68, 0.2); color: #fca5a5;">invalidated</span>
                {/if}
                <div style="margin-left: auto; display: flex; gap: 4px;">
                  <label class="toggle" title="isImportant">
                    <input
                      type="checkbox"
                      checked={!!summary.isImportant}
                      onchange={async () => {
                        const next = { ...summary, isImportant: !summary.isImportant };
                        hypaSummaries = [...hypaSummaries.slice(0, idx), next, ...hypaSummaries.slice(idx + 1)];
                        await saveHypa();
                      }}
                    />
                    <span class="toggle-track"></span>
                  </label>
                  <button class="btn btn-sm btn-secondary" onclick={() => openModal(idx)}>View</button>
                  <button class="btn btn-sm btn-danger" onclick={() => deleteSummary(idx)}>Delete</button>
                </div>
              </div>
              <p style="font-size: 12px; color: var(--fg); line-height: 1.4; margin: 0;">
                {summaryPreview(summary.text)}
              </p>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
{/if}

<HypaModal
  summary={modalSummary}
  onClose={closeModal}
  onToggleImportant={toggleImportantOnModal}
/>
