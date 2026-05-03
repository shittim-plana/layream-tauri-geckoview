<script>
  import { invoke } from "../lib/tauri.js";
  import FileImport from "../components/FileImport.svelte";
  import CBSEditor from "../components/CBSEditor.svelte";

  let preset = $state(null);
  let presetName = $state("");
  let loading = $state(false);
  let error = $state("");

  let subTab = $state("prompts");
  let editingIndex = $state(-1);
  let previewText = $state("");
  let showPreview = $state(false);

  const TYPE_COLORS = {
    plain: "var(--type-plain)", jailbreak: "var(--type-jailbreak)", cot: "var(--type-cot)",
    chat: "var(--type-chat)", description: "var(--type-description)", persona: "var(--type-persona)",
    lorebook: "var(--type-lorebook)", authornote: "var(--type-authornote)", memory: "var(--type-memory)",
    postEverything: "var(--type-post)", cache: "var(--type-cache)", chatML: "var(--type-plain)",
  };

  const PROMPT_TYPES = ["plain", "jailbreak", "cot", "chat", "description", "persona", "lorebook", "authornote", "memory", "postEverything", "cache"];
  const ROLES = ["system", "user", "bot"];

  const PRESET_EXTS = [".risup", ".risupreset", ".json", ".preset"];

  // RisuAI uses -1000 as sentinel for "disabled/default" parameters
  const PARAM_SENTINEL = -1000;
  function displayParam(v) { return v === PARAM_SENTINEL ? "" : v; }
  function parseParam(v) { return v === "" || v === undefined ? PARAM_SENTINEL : Number(v); }

  async function handleFile(name, data) {
    const ext = "." + name.split(".").pop()?.toLowerCase();
    if (!PRESET_EXTS.includes(ext)) {
      error = `지원하지 않는 형식: ${ext} (${PRESET_EXTS.join(", ")} 만 가능)`;
      return;
    }
    loading = true;
    error = "";
    try {
      const result = await invoke("load_preset", { name, data });
      if (result) {
        preset = result;
        presetName = name;
        editingIndex = -1;
        error = "";
      } else {
        error = "load_preset returned null/undefined";
      }
    } catch (e) {
      error = `load_preset error: ${String(e)}`;
    }
    loading = false;
  }

  async function exportPreset(format) {
    if (!preset) return;
    try {
      const result = await invoke("export_preset", { preset, format });
      if (result) {
        const fileName = `${preset.name || "preset"}.${result.ext}`;
        const data = new Uint8Array(result.data);
        // Try Tauri fs plugin (works on Android where Blob URLs are blocked)
        try {
          const { writeFile, BaseDirectory } = await import("@tauri-apps/plugin-fs");
          await writeFile(fileName, data, { baseDir: BaseDirectory.Download });
          error = `Saved to Downloads/${fileName}`;
        } catch (fsErr) {
          // Fallback: Blob URL download (works on desktop)
          const blob = new Blob([data]);
          const url = URL.createObjectURL(blob);
          const a = document.createElement("a");
          a.href = url;
          a.download = fileName;
          a.click();
          URL.revokeObjectURL(url);
        }
      }
    } catch (e) {
      error = String(e);
    }
  }

  function closePreset() {
    preset = null;
    presetName = "";
    editingIndex = -1;
    error = "";
  }

  function getItemText(item) {
    return item?.text ?? item?.innerFormat ?? "";
  }

  function setItemText(item, text) {
    if ("text" in item) item.text = text;
    else if ("innerFormat" in item) item.innerFormat = text;
    else item.text = text;
  }

  function addPromptItem() {
    if (!preset) return;
    if (!preset.promptTemplate) preset.promptTemplate = [];
    preset.promptTemplate = [...preset.promptTemplate, { type: "plain", role: "system", text: "" }];
    editingIndex = preset.promptTemplate.length - 1;
  }

  function deletePromptItem(idx) {
    if (!preset?.promptTemplate) return;
    preset.promptTemplate = preset.promptTemplate.filter((_, i) => i !== idx);
    if (editingIndex >= preset.promptTemplate.length) editingIndex = -1;
  }

  async function updatePreview() {
    if (!preset?.promptTemplate?.[editingIndex]) return;
    const text = getItemText(preset.promptTemplate[editingIndex]);
    try {
      previewText = await invoke("evaluate_cbs", { input: text, char_name: "Character", user_name: "User" });
    } catch (e) {
      previewText = `Error: ${e}`;
    }
  }
</script>

<div>
  {#if error}
    <div class="card" style="border-color: var(--red); color: var(--red);">
      <div class="card-body">{error}</div>
    </div>
  {/if}

  {#if !preset}
    <div class="empty-state animate-in">
      <FileImport
        accept="application/json,application/octet-stream"
        label="프리셋 파일 불러오기"
        extensions=".risup, .risupreset, .json, .preset"
        onfile={handleFile}
        disabled={loading}
      />
      {#if loading}
        <div class="spinner"></div>
      {/if}
    </div>
  {:else}
    <!-- Preset Header -->
    <div class="card">
      <div class="card-header">
        <span class="card-title">{presetName}</span>
        <div style="display: flex; gap: 6px;">
          <button class="btn btn-sm btn-secondary" onclick={() => exportPreset("risup")}>Export .risup</button>
          <button class="btn btn-sm btn-secondary" onclick={() => exportPreset("json")}>Export .json</button>
          <button class="btn btn-sm btn-danger" onclick={closePreset}>Close</button>
        </div>
      </div>
    </div>

    <!-- Sub-tabs -->
    <div class="tab-bar">
      <button class="tab-btn" class:active={subTab === "prompts"} onclick={() => { subTab = "prompts"; editingIndex = -1; }}>Prompts</button>
      <button class="tab-btn" class:active={subTab === "regex"} onclick={() => subTab = "regex"}>Regex</button>
      <button class="tab-btn" class:active={subTab === "params"} onclick={() => subTab = "params"}>Parameters</button>
    </div>

    <!-- Prompts Tab -->
    {#if subTab === "prompts"}
      {#if editingIndex >= 0 && preset.promptTemplate?.[editingIndex]}
        {@const item = preset.promptTemplate[editingIndex]}
        <!-- Back bar -->
        <div style="display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px;">
          <button class="btn btn-sm btn-secondary" onclick={() => editingIndex = -1}>← Back</button>
          <span style="font-size: 12px; color: var(--fg3);">{editingIndex + 1}/{preset.promptTemplate.length}</span>
          <div style="display: flex; gap: 4px;">
            <button class="btn btn-sm btn-secondary" disabled={editingIndex <= 0} onclick={() => editingIndex--}>Prev</button>
            <button class="btn btn-sm btn-secondary" disabled={editingIndex >= preset.promptTemplate.length - 1} onclick={() => editingIndex++}>Next</button>
          </div>
        </div>

        <div class="card">
          <div class="card-header">
            <span class="prompt-type-badge" style="background: {TYPE_COLORS[item.type] || 'var(--bg4)'};">{item.type}</span>
            <div style="display: flex; gap: 6px; align-items: center;">
              <select class="select" style="width: auto; padding: 4px 8px; font-size: 12px;" bind:value={item.type}>
                {#each PROMPT_TYPES as t}<option value={t}>{t}</option>{/each}
              </select>
              {#if "role" in item}
                <select class="select" style="width: auto; padding: 4px 8px; font-size: 12px;" bind:value={item.role}>
                  {#each ROLES as r}<option value={r}>{r}</option>{/each}
                </select>
              {/if}
              <button class="btn btn-sm btn-danger" onclick={() => { deletePromptItem(editingIndex); editingIndex = -1; }}>Delete</button>
            </div>
          </div>
          <div class="card-body">
            <CBSEditor
              value={getItemText(item)}
              onchange={(text) => setItemText(item, text)}
            />
            <div style="margin-top: 8px; display: flex; gap: 6px;">
              <button class="btn btn-sm btn-secondary" onclick={() => { showPreview = !showPreview; if (showPreview) updatePreview(); }}>
                {showPreview ? "Hide Preview" : "Preview"}
              </button>
            </div>
            {#if showPreview}
              <div class="preview" style="margin-top: 8px;">{previewText}</div>
            {/if}
          </div>
        </div>
      {:else}
        <!-- Prompt list -->
        <div class="card">
          <div class="card-header">
            <span class="card-title">Prompt Template ({preset.promptTemplate?.length || 0} items)</span>
            <button class="btn btn-sm btn-primary" onclick={addPromptItem}>+ Add</button>
          </div>
          {#if preset.promptTemplate?.length}
            <ul class="prompt-list">
              {#each preset.promptTemplate as item, i}
                <li class="prompt-item" onclick={() => editingIndex = i}>
                  <span class="prompt-type-badge" style="background: {TYPE_COLORS[item.type] || 'var(--bg4)'};">{item.type}</span>
                  <span class="prompt-item-text">
                    {getItemText(item)?.slice(0, 60) || "(empty)"}{(getItemText(item)?.length || 0) > 60 ? "..." : ""}
                  </span>
                </li>
              {/each}
            </ul>
          {:else}
            <div class="card-body" style="text-align: center; color: var(--fg3);">
              No prompt template items. Click + Add to create one.
            </div>
          {/if}
        </div>
      {/if}
    {/if}

    <!-- Regex Tab -->
    {#if subTab === "regex"}
      <div class="card">
        <div class="card-header">
          <span class="card-title">Regex Scripts ({preset.regex?.length || 0})</span>
          <button class="btn btn-sm btn-primary" onclick={() => {
            if (!preset.regex) preset.regex = [];
            preset.regex = [...preset.regex, { comment: "", in: "", out: "", type: "editinput" }];
          }}>+ Add</button>
        </div>
        {#if preset.regex?.length}
          <div class="card-body">
            {#each preset.regex as rule, i}
              <div style="padding: 10px 0; border-bottom: 1px solid var(--bg4);">
                <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 6px;">
                  <input class="input" style="flex:1;" type="text" bind:value={rule.comment} placeholder="Comment" />
                  <button class="btn-icon" style="color: var(--red);" onclick={() => {
                    preset.regex = preset.regex.filter((_, j) => j !== i);
                  }}>✕</button>
                </div>
                <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 6px;">
                  <div class="field">
                    <label class="label">Pattern</label>
                    <input class="input" type="text" bind:value={rule.in} placeholder="/regex/flags" />
                  </div>
                  <div class="field">
                    <label class="label">Replacement</label>
                    <input class="input" type="text" bind:value={rule.out} placeholder="replacement" />
                  </div>
                </div>
              </div>
            {/each}
          </div>
        {:else}
          <div class="card-body" style="text-align: center; color: var(--fg3);">
            No regex rules defined.
          </div>
        {/if}
      </div>
    {/if}

    <!-- Parameters Tab -->
    {#if subTab === "params"}
      <div class="card">
        <div class="card-header"><span class="card-title">Basic Parameters</span></div>
        <div class="card-body">
          <div class="field">
            <label class="label">Preset Name</label>
            <input class="input" type="text" bind:value={preset.name} />
          </div>
          {#if preset.aiModel !== undefined}
            <div class="field">
              <label class="label">AI Model</label>
              <input class="input" type="text" bind:value={preset.aiModel} />
            </div>
          {/if}
          <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 8px;">
            <div class="field">
              <label class="label">Temperature</label>
              <input class="input" type="number" step="0.1" bind:value={preset.temperature} />
            </div>
            <div class="field">
              <label class="label">Max Context</label>
              <input class="input" type="number" bind:value={preset.maxContext} />
            </div>
            <div class="field">
              <label class="label">Max Response</label>
              <input class="input" type="number" bind:value={preset.maxResponse} />
            </div>
            <div class="field">
              <label class="label">Top P</label>
              <input class="input" type="number" step="0.01" value={displayParam(preset.top_p)}
                oninput={(e) => preset.top_p = parseParam(e.target.value)} />
            </div>
            <div class="field">
              <label class="label">Top K</label>
              <input class="input" type="number" value={displayParam(preset.top_k)}
                oninput={(e) => preset.top_k = parseParam(e.target.value)} />
            </div>
            <div class="field">
              <label class="label">Min P</label>
              <input class="input" type="number" step="0.01" value={displayParam(preset.min_p)}
                oninput={(e) => preset.min_p = parseParam(e.target.value)} />
            </div>
            <div class="field">
              <label class="label">Freq Penalty</label>
              <input class="input" type="number" step="0.1" value={displayParam(preset.frequencyPenalty)}
                oninput={(e) => preset.frequencyPenalty = parseParam(e.target.value)} />
            </div>
            <div class="field">
              <label class="label">Presence Penalty</label>
              <input class="input" type="number" step="0.1" value={displayParam(preset.PresensePenalty)}
                oninput={(e) => preset.PresensePenalty = parseParam(e.target.value)} />
            </div>
          </div>
        </div>
      </div>

      <div class="card">
        <div class="card-header"><span class="card-title">Prompt Settings</span></div>
        <div class="card-body">
          <div class="field">
            <label class="label">Main Prompt</label>
            <textarea class="textarea" rows="4" bind:value={preset.mainPrompt}></textarea>
          </div>
          <div class="field">
            <label class="label">Jailbreak</label>
            <textarea class="textarea" rows="4" bind:value={preset.jailbreak}></textarea>
          </div>
          <div class="field">
            <label class="label">Global Note</label>
            <textarea class="textarea" rows="3" bind:value={preset.globalNote}></textarea>
          </div>
          {#if preset.promptSettings?.assistantPrefill !== undefined}
            <div class="field">
              <label class="label">Assistant Prefill</label>
              <textarea class="textarea" rows="2" bind:value={preset.promptSettings.assistantPrefill}></textarea>
            </div>
          {/if}
        </div>
      </div>
    {/if}
  {/if}
</div>
