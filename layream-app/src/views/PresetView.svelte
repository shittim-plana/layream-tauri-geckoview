<script>
  import { onMount } from "svelte";
  import { invoke } from "../lib/tauri.js";
  import FileImport from "../components/FileImport.svelte";
  import CBSEditor from "../components/CBSEditor.svelte";

  let preset = $state(null);
  let presetName = $state("");
  let loading = $state(false);
  let error = $state("");
  let status = $state("");

  let subTab = $state("prompts");
  let editingIndex = $state(-1);
  let previewText = $state("");
  let showPreview = $state(false);

  // CBS preview context — loaded once on mount; falls back to placeholder
  // names so previews remain meaningful when no character/settings are saved.
  let charName = $state("Character");
  let userName = $state("User");

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

  async function handleFile(name, data, tempName) {
    const ext = "." + name.split(".").pop()?.toLowerCase();
    if (!PRESET_EXTS.includes(ext)) {
      error = `지원하지 않는 형식: ${ext} (${PRESET_EXTS.join(", ")} 만 가능)`;
      return;
    }
    loading = true;
    error = "";
    try {
      const result = tempName
        ? await invoke("load_preset_from_path", { name, temp_name: tempName })
        : await invoke("load_preset", { name, data });
      if (result) {
        preset = result;
        presetName = name;
        editingIndex = -1;
        error = "";
        invoke("cmd_save_current_preset", { preset: result }).catch(e => { console.error("Auto-save failed:", e); status = "Auto-save failed"; });
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
        try {
          const savedPath = await invoke("save_file_to_downloads", {
            filename: fileName,
            data: Array.from(data),
          });
          status = `Saved to ${savedPath}`;
          setTimeout(() => { if (status?.startsWith("Saved to")) status = ""; }, 4000);
        } catch (saveErr) {
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
    status = "";
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
    const toggles = {};
    if (preset.customPromptTemplateToggle) {
      for (const line of preset.customPromptTemplateToggle.split("\n")) {
        const m = line.match(/^(\w+)\s*[:=]\s*(.+)$/);
        if (m) toggles[m[1]] = m[2].trim();
      }
    }
    if (preset.templateDefaultVariables) {
      for (const line of preset.templateDefaultVariables.split("\n")) {
        const m = line.match(/^(\w+)\s*[:=]\s*(.+)$/);
        if (m) toggles[m[1]] = m[2].trim();
      }
    }
    try {
      previewText = await invoke("evaluate_cbs", { input: text, char_name: charName, user_name: userName, toggles });
    } catch (e) {
      previewText = `Error: ${e}`;
    }
  }

  async function savePreset() {
    if (!preset) return;
    try {
      await invoke("cmd_save_current_preset", { preset });
      status = "Saved!";
      setTimeout(() => { if (status === "Saved!") status = ""; }, 2000);
    } catch (e) {
      error = `Save failed: ${String(e)}`;
    }
  }

  onMount(async () => {
    try {
      const saved = await invoke("cmd_load_current_preset");
      if (saved && typeof saved === "object" && Object.keys(saved).length > 0) {
        preset = saved;
        presetName = saved.name || "Saved Preset";
      }
    } catch (e) {
      console.error("Load preset failed:", e);
    }
    try {
      const ch = await invoke("cmd_load_current_character");
      const cardName = ch?.card?.data?.name || ch?.card?.name;
      if (typeof cardName === "string" && cardName.length > 0) charName = cardName;
    } catch (e) {
      console.error("Load character for CBS preview failed:", e);
    }
    try {
      const settings = await invoke("cmd_load_settings");
      if (typeof settings?.userName === "string" && settings.userName.length > 0) {
        userName = settings.userName;
      }
    } catch (e) {
      console.error("Load settings for CBS preview failed:", e);
    }
  });
</script>

<div>
  {#if error}
    <div class="card" style="border-color: var(--red); color: var(--red);">
      <div class="card-body">{error}</div>
    </div>
  {/if}

  {#if status}
    <div class="card" style="border-color: var(--accent); color: var(--accent);">
      <div class="card-body">{status}</div>
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
          <button class="btn btn-sm btn-primary" onclick={savePreset}>Save</button>
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
            <button class="btn btn-sm btn-secondary" disabled={editingIndex <= 0} onclick={() => { editingIndex = editingIndex - 1; }}>Prev</button>
            <button class="btn btn-sm btn-secondary" disabled={editingIndex >= preset.promptTemplate.length - 1} onclick={() => { editingIndex = editingIndex + 1; }}>Next</button>
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
              <input class="input" type="number" step="0.01"
                value={preset.temperature != null ? (preset.temperature / 100).toFixed(2) : ""}
                oninput={(e) => preset.temperature = Math.round(Number(e.target.value) * 100)} />
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
              <input class="input" type="number" step="0.01" placeholder="미설정" value={displayParam(preset.top_p)}
                oninput={(e) => preset.top_p = parseParam(e.target.value)} />
            </div>
            <div class="field">
              <label class="label">Top K</label>
              <input class="input" type="number" placeholder="미설정" value={displayParam(preset.top_k)}
                oninput={(e) => preset.top_k = parseParam(e.target.value)} />
            </div>
            <div class="field">
              <label class="label">Min P</label>
              <input class="input" type="number" step="0.01" placeholder="미설정" value={displayParam(preset.min_p)}
                oninput={(e) => preset.min_p = parseParam(e.target.value)} />
            </div>
            <div class="field">
              <label class="label">Top A</label>
              <input class="input" type="number" step="0.01" placeholder="미설정" value={displayParam(preset.top_a)}
                oninput={(e) => preset.top_a = parseParam(e.target.value)} />
            </div>
            <div class="field">
              <label class="label">Repetition Penalty</label>
              <input class="input" type="number" step="0.01" placeholder="미설정" value={displayParam(preset.repetition_penalty)}
                oninput={(e) => preset.repetition_penalty = parseParam(e.target.value)} />
            </div>
            <div class="field">
              <label class="label">Freq Penalty</label>
              <input class="input" type="number" step="0.1" placeholder="미설정" value={displayParam(preset.frequencyPenalty)}
                oninput={(e) => preset.frequencyPenalty = parseParam(e.target.value)} />
            </div>
            <div class="field">
              <label class="label">Presence Penalty</label>
              <input class="input" type="number" step="0.1" placeholder="미설정" value={displayParam(preset.PresensePenalty)}
                oninput={(e) => preset.PresensePenalty = parseParam(e.target.value)} />
            </div>
            <div class="field">
              <label class="label">Reason Effort</label>
              <input class="input" type="number" step="0.1" placeholder="미설정" value={displayParam(preset.reasonEffort)}
                oninput={(e) => preset.reasonEffort = parseParam(e.target.value)} />
            </div>
            <div class="field">
              <label class="label">Thinking Tokens</label>
              <input class="input" type="number" placeholder="미설정" value={displayParam(preset.thinkingTokens)}
                oninput={(e) => preset.thinkingTokens = parseParam(e.target.value)} />
            </div>
          </div>
        </div>
      </div>

      <div class="card">
        <div class="card-header"><span class="card-title">Custom Flags</span></div>
        <div class="card-body">
          <label style="display: flex; align-items: center; gap: 8px; margin-bottom: 8px;">
            <input type="checkbox" bind:checked={preset.enableCustomFlags} />
            Enable Custom Flags
          </label>
          {#if preset.enableCustomFlags}
            {@const FLAG_NAMES = ["hasImageInput","hasImageOutput","hasAudioInput","hasAudioOutput","hasPrefill","hasCache","hasFullSystemPrompt","hasFirstSystemPrompt","hasStreaming","requiresAlternateRole","mustStartWithUserInput","poolSupported","hasVideoInput","OAICompletionTokens","DeveloperRole","geminiThinking","geminiBlockOff","deepSeekPrefix","deepSeekThinkingInput","deepSeekThinkingOutput","noCivilIntegrity","claudeThinking"]}
            {@const displayFlags = Array.isArray(preset.customFlags) ? preset.customFlags.map(f => typeof f === "number" ? (FLAG_NAMES[f] || `#${f}`) : f) : []}
            <textarea class="textarea" rows="4"
              placeholder="한 줄에 하나씩 플래그 입력&#10;예: hasPrefill&#10;geminiThinking&#10;hasStreaming"
              value={displayFlags.join("\n")}
              oninput={(e) => {
                preset.customFlags = e.target.value.split("\n").map(s => s.trim()).filter(Boolean).map(s => {
                  const idx = FLAG_NAMES.indexOf(s);
                  const originals = preset.customFlags || [];
                  const hasNumbers = originals.some(f => typeof f === "number");
                  return hasNumbers && idx >= 0 ? idx : s;
                });
              }}
            ></textarea>
            <p style="font-size: 11px; color: var(--fg3); margin-top: 4px;">
              {preset.customFlags?.length || 0}개 플래그 설정됨
            </p>
          {/if}
        </div>
      </div>

      {#if !preset.promptTemplate?.length}
      <div class="card">
        <div class="card-header"><span class="card-title">Legacy Prompt Fields</span></div>
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
        </div>
      </div>
      {/if}

      {#if preset.promptSettings}
      <div class="card">
        <div class="card-header"><span class="card-title">프롬프트 설정</span></div>
        <div class="card-body">
          <div class="field">
            <label class="label">응답 시작 텍스트 (Prefill)</label>
            <p style="font-size: 11px; color: var(--fg3); margin: 0 0 4px;">AI 응답의 첫 부분을 미리 채워넣습니다</p>
            <textarea class="textarea" rows="2" bind:value={preset.promptSettings.assistantPrefill} placeholder="예: (내면의 생각을 정리하며)"></textarea>
          </div>
          <div class="field">
            <label class="label">프롬프트 종결 형식</label>
            <p style="font-size: 11px; color: var(--fg3); margin: 0 0 4px;">프롬프트 마지막에 추가되는 형식 문자열</p>
            <input class="input" type="text" bind:value={preset.promptSettings.postEndInnerFormat} placeholder="비어있으면 기본값" />
          </div>
          <div style="display: flex; flex-direction: column; gap: 10px; margin-top: 12px;">
            <label style="display: flex; align-items: flex-start; gap: 8px;">
              <input type="checkbox" style="margin-top: 3px;" bind:checked={preset.promptSettings.sendChatAsSystem} />
              <span>
                <span style="font-size: 13px;">채팅을 시스템 메시지로 전송</span>
                <span style="display: block; font-size: 11px; color: var(--fg3);">대화 내역을 user/assistant 대신 system role로 보냅니다</span>
              </span>
            </label>
            <label style="display: flex; align-items: flex-start; gap: 8px;">
              <input type="checkbox" style="margin-top: 3px;" bind:checked={preset.promptSettings.sendName} />
              <span>
                <span style="font-size: 13px;">메시지에 이름 포함</span>
                <span style="display: block; font-size: 11px; color: var(--fg3);">각 메시지에 캐릭터/사용자 이름을 추가합니다</span>
              </span>
            </label>
            <label style="display: flex; align-items: flex-start; gap: 8px;">
              <input type="checkbox" style="margin-top: 3px;" bind:checked={preset.promptSettings.utilOverride} />
              <span>
                <span style="font-size: 13px;">유틸리티 봇 템플릿 덮어쓰기</span>
                <span style="display: block; font-size: 11px; color: var(--fg3);">유틸리티 봇이어도 이 프리셋의 promptTemplate을 사용합니다</span>
              </span>
            </label>
            <label style="display: flex; align-items: flex-start; gap: 8px;">
              <input type="checkbox" style="margin-top: 3px;" bind:checked={preset.promptSettings.customChainOfThought} />
              <span>
                <span style="font-size: 13px;">사고 과정 (CoT) 커스텀 처리</span>
                <span style="display: block; font-size: 11px; color: var(--fg3);">thinking 태그를 직접 관리합니다</span>
              </span>
            </label>
          </div>
          {#if preset.promptSettings.customChainOfThought}
          <div class="field" style="margin-top: 8px;">
            <label class="label">사고 태그 최대 깊이</label>
            <input class="input" type="number" min="1" bind:value={preset.promptSettings.maxThoughtTagDepth} placeholder="기본값: 1" />
          </div>
          {/if}
        </div>
      </div>
      {/if}

      {#if preset.customPromptTemplateToggle !== undefined}
      <div class="card">
        <div class="card-header"><span class="card-title">토글 정의 (customPromptTemplateToggle)</span></div>
        <div class="card-body">
          <textarea class="textarea" rows="6" bind:value={preset.customPromptTemplateToggle}></textarea>
        </div>
      </div>
      {/if}

      {#if preset.templateDefaultVariables !== undefined}
      <div class="card">
        <div class="card-header"><span class="card-title">기본 변수 (templateDefaultVariables)</span></div>
        <div class="card-body">
          <textarea class="textarea" rows="4" placeholder="key=value&#10;key2=value2" bind:value={preset.templateDefaultVariables}></textarea>
        </div>
      </div>
      {/if}
    {/if}
  {/if}
</div>
