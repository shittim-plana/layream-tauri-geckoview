<script>
  import { invoke } from "../lib/tauri.js";

  let preset = $state(null);
  let presetName = $state("");
  let loading = $state(false);
  let error = $state("");

  let editingField = $state(null);

  async function loadPreset() {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".risup,.risupreset,.json,.preset";
    input.onchange = async () => {
      const file = input.files?.[0];
      if (!file) return;
      loading = true;
      error = "";
      try {
        const buf = await file.arrayBuffer();
        const data = Array.from(new Uint8Array(buf));
        const result = await invoke("load_preset", { name: file.name, data });
        if (result) {
          preset = result;
          presetName = file.name;
        }
      } catch (e) {
        error = String(e);
      }
      loading = false;
    };
    input.click();
  }

  async function exportPreset(format) {
    if (!preset) return;
    try {
      const result = await invoke("export_preset", { preset, format });
      if (result) {
        const blob = new Blob([new Uint8Array(result.data)]);
        const url = URL.createObjectURL(blob);
        const a = document.createElement("a");
        a.href = url;
        a.download = `${preset.name || "preset"}.${result.ext}`;
        a.click();
        URL.revokeObjectURL(url);
      }
    } catch (e) {
      error = String(e);
    }
  }

  const fields = [
    { key: "mainPrompt", label: "Main Prompt", type: "textarea" },
    { key: "jailbreak", label: "Jailbreak", type: "textarea" },
    { key: "globalNote", label: "Global Note", type: "textarea" },
    { key: "temperature", label: "Temperature", type: "number" },
    { key: "maxContext", label: "Max Context", type: "number" },
    { key: "maxResponse", label: "Max Response", type: "number" },
    { key: "frequencyPenalty", label: "Frequency Penalty", type: "number" },
    { key: "PresensePenalty", label: "Presence Penalty", type: "number" },
  ];
</script>

<div>
  <div style="display: flex; gap: 8px; margin-bottom: 12px;">
    <button onclick={loadPreset} disabled={loading}>
      {loading ? "Loading..." : "Load Preset"}
    </button>
    {#if preset}
      <button onclick={() => exportPreset("risup")}>Export .risup</button>
      <button onclick={() => exportPreset("json")}>Export .json</button>
    {/if}
  </div>

  {#if error}
    <div class="card" style="border-color: var(--accent); color: var(--accent);">
      {error}
    </div>
  {/if}

  {#if preset}
    <div class="card">
      <div class="field">
        <label>Preset Name</label>
        <input type="text" bind:value={preset.name} />
      </div>

      {#if preset.aiModel}
        <div class="field">
          <label>AI Model</label>
          <input type="text" bind:value={preset.aiModel} />
        </div>
      {/if}
    </div>

    {#each fields as field}
      <div class="card">
        <div class="field">
          <label>{field.label}</label>
          {#if field.type === "textarea"}
            <textarea
              rows="4"
              value={preset[field.key] || ""}
              oninput={(e) => (preset[field.key] = e.target.value)}
            ></textarea>
          {:else}
            <input
              type="number"
              value={preset[field.key] || 0}
              oninput={(e) => (preset[field.key] = Number(e.target.value))}
            />
          {/if}
        </div>
      </div>
    {/each}

    {#if preset.promptTemplate?.length}
      <div class="card">
        <label style="margin-bottom: 8px; display: block;">
          Prompt Template ({preset.promptTemplate.length} items)
        </label>
        {#each preset.promptTemplate as item, i}
          <div style="padding: 4px 0; border-bottom: 1px solid var(--border); font-size: 13px;">
            <span style="color: var(--accent);">{item.type}</span>
            {#if item.text}
              <span style="color: var(--text-dim); margin-left: 8px;">
                {item.text.slice(0, 60)}{item.text.length > 60 ? "..." : ""}
              </span>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  {:else}
    <div class="card" style="text-align: center; padding: 48px;">
      <p style="color: var(--text-dim);">Load a preset file to begin editing</p>
      <p style="font-size: 12px; color: var(--text-dim); margin-top: 8px;">
        Supports .risup, .risupreset, .json
      </p>
    </div>
  {/if}
</div>
