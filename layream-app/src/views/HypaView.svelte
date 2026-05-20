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
  const STATUS_CLEAR_MS = 3000;
  let modalSummary = $state(null);
  let modalIndex = $state(-1);
  function getMinSummarizedAt() {
    return (hypaSummaries.length || 0) * (Number(hypaSummaryUnit) || 10);
  }
  // app-flush listener cleanup handle. Set in onMount, called in onDestroy.
  let unlistenAppFlush;

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
      console.error("Failed to load HyPA settings:", e);
      hypaSettingsLoaded = true;
    }
    onReady?.({
      loadHypa,
      loadAll,
      triggerSummarizationIfNeeded,
      getRagContext,
    });

    // app-flush listener: when App.svelte broadcasts "app-flush" before
    // window destroy, persist any pending settings/summaries immediately,
    // bypassing the 500ms debounce. The 500ms grace window in App.svelte
    // is only meaningful if listeners actually save here.
    try {
      const { listen } = await import("@tauri-apps/api/event");
      unlistenAppFlush = await listen("app-flush", async () => {
        clearTimeout(hypaSettingsSaveTimeout);
        await Promise.all([flushSettingsSave(), saveHypa()]);
      });
    } catch (e) {
      console.error("HypaView app-flush listener unavailable:", e);
    }
  });

  onDestroy(() => {
    clearTimeout(hypaSettingsSaveTimeout);
    if (unlistenAppFlush) unlistenAppFlush();
  });

  // Single save path — called via either debounced timer (typing) or
  // immediate flush on app-close. Idempotent: safe to call multiple times.
  async function flushSettingsSave() {
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
    } catch (e) { console.error("Failed to save HyPA settings:", e); }
  }

  function scheduleHypaSettingsSave() {
    clearTimeout(hypaSettingsSaveTimeout);
    hypaSettingsSaveTimeout = setTimeout(flushSettingsSave, 500);
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

  async function loadAll() {
    await loadHypa();
    return hypaSummaries;
  }

  async function loadHypa() {
    try {
      // hypa_load_all returns the raw hypa.json `{ summaries: [...] }`.
      // Field shape matches RisuAI HypaV3 (chatMemos/isImportant preserved
      // by serde rename in commands_hypa.rs Summary struct).
      const data = await invoke("hypa_load_all");
      hypaSummaries = data?.summaries || [];
      hypaMemoryCount = hypaSummaries.length;
    } catch (e) { console.error("Failed to load HyPA:", e); }
  }

  async function saveHypa() {
    try {
      // hypa_save_all takes a single `summaries: Value` arg — we pass the
      // full hypa.json object (containing `summaries` key) so the file is
      // overwritten atomically. Outer key matches the Rust parameter name.
      await invoke("hypa_save_all", { summaries: { summaries: hypaSummaries } });
    } catch (e) { console.error("Failed to save HyPA:", e); }
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
      // Try extraction in order of specificity:
      // 1. { summaries: [...] } — Layream export / RisuAI SerializableHypaV3Data
      // 2. bare array [...]
      // 3. { hypaV3Data: { summaries: [...] } } — RisuAI chat export wrapper
      const items = data?.summaries
        ?? (Array.isArray(data) ? data : null)
        ?? data?.hypaV3Data?.summaries
        ?? null;
      if (items && Array.isArray(items)) {
        hypaSummaries = items;
        hypaMemoryCount = hypaSummaries.length;
        await saveHypa();
        hypaImportStatus = `imported ${hypaSummaries.length} summaries`;
      } else {
        hypaImportStatus = `invalid format — expected { summaries: [...] }, [...], or { hypaV3Data: { summaries: [...] } }`;
      }
    } catch (e) { hypaImportStatus = `import error: ${e}`; }
    setTimeout(() => { hypaImportStatus = ""; }, STATUS_CLEAR_MS);
  }

  function clearHypa() {
    if (!confirm("Clear ALL summaries? This cannot be undone.")) return;
    hypaSummaries = [];
    hypaMemoryCount = 0;
    saveHypa();
  }

  // ------- Summary list actions -------

  function openModal(index) {
    if (index < 0 || index >= hypaSummaries.length) {
      modalSummary = null;
      modalIndex = -1;
      return;
    }
    modalIndex = index;
    modalSummary = hypaSummaries[index];
  }

  function closeModal() {
    modalSummary = null;
    modalIndex = -1;
  }

  async function toggleImportantOnModal() {
    if (!modalSummary || modalIndex < 0 || modalIndex >= hypaSummaries.length) return;
    const next = { ...hypaSummaries[modalIndex], isImportant: !hypaSummaries[modalIndex].isImportant };
    hypaSummaries = [...hypaSummaries.slice(0, modalIndex), next, ...hypaSummaries.slice(modalIndex + 1)];
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
    try {
      await loadHypa();
      hypaActionStatus = `refreshed ${hypaSummaries.length} summaries`;
    } catch (e) {
      hypaActionStatus = `refresh failed: ${e}`;
    }
    setTimeout(() => { hypaActionStatus = ""; }, STATUS_CLEAR_MS);
  }

  async function cleanupHypa() {
    hypaActionStatus = "cleaning up...";
    // Try backend cleanup first (returns count of removed entries).
    try {
      const removed = await invoke("hypa_cleanup");
      hypaActionStatus = `removed ${removed ?? 0} empty summaries`;
      // Reload after backend mutation.
      await loadHypa();
      setTimeout(() => { hypaActionStatus = ""; }, STATUS_CLEAR_MS);
      return;
    } catch (e) {
      console.error("hypa_cleanup failed, doing local cleanup:", e);
    }
    // Fallback: drop locally-empty summaries (chatMemos empty).
    const before = hypaSummaries.length;
    hypaSummaries = hypaSummaries.filter(
      (s) => Array.isArray(s.chatMemos) && s.chatMemos.length > 0
    );
    hypaMemoryCount = hypaSummaries.length;
    await saveHypa();
    hypaActionStatus = `removed ${before - hypaSummaries.length} empty summaries (local)`;
    setTimeout(() => { hypaActionStatus = ""; }, STATUS_CLEAR_MS);
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
    // Fire only when crossing a unit boundary AND we haven't already
    // summarized past this point. Derived from persisted hypaSummaries.length
    // so the gate survives component remount.
    if (messages.length % unit !== 0) return null;
    if (messages.length <= getMinSummarizedAt()) return null;
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
      // Backend (commands_hypa.rs:320) returns the Summary but does NOT
      // persist — caller assigns chatMemos from message ids and saves.
      // Without this step, hypa.json is never written and summaries are lost.
      if (!result || typeof result !== "object") return null;
      const memos = slice.map((m) => m && m.chatId).filter(Boolean);
      const summary = { ...result, chatMemos: memos };
      hypaSummaries = [...hypaSummaries, summary];
      hypaMemoryCount = hypaSummaries.length;
      await saveHypa();
      return summary;
    } catch (e) {
      console.error("hypa_summarize failed:", e);
      hypaActionStatus = `Summarization failed: ${e}`;
      setTimeout(() => { hypaActionStatus = ""; }, STATUS_CLEAR_MS);
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
      // §3-D: Rust signature is `query_embedding: Vec<f64>, top_k: usize` —
      // Tauri matches arg names verbatim (no auto camelCase conversion in this
      // project's invoke wrapper, see lib/tauri.js). camelCase keys here would
      // arrive as `None` in Rust → "missing argument" error.
      const results = await invoke("hypa_search", {
        query_embedding: queryEmbedding,
        top_k: topK,
      });
      return Array.isArray(results) ? results : [];
    } catch (e) {
      console.error("hypa_search failed:", e);
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
          <!-- Key uses array index because summaries lack a stable unique id.
               Acceptable: delete/reorder triggers full re-render of shifted items. -->
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
