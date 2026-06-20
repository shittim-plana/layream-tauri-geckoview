<script>
  import { invoke } from "../lib/tauri.js";
  import { toUserError } from "../lib/errors.js";
  import ResizableTextarea from "../components/ResizableTextarea.svelte";

  let { moduleId = "", moduleName = "", onBack = () => {} } = $props();

  let loading = $state(true);
  let saving = $state(false);
  let error = $state("");
  let status = $state("");

  // Module data — the raw JSON value from library
  let moduleData = $state(null);

  // Editable fields extracted from module.module
  let name = $state("");
  let description = $state("");
  let lorebook = $state([]);
  let regex = $state([]);

  // UI state
  let expandedLore = $state(null); // index of expanded lorebook entry
  let expandedRegex = $state(null); // index of expanded regex entry

  function flashStatus(msg) {
    status = msg;
    setTimeout(() => { if (status === msg) status = ""; }, 2000);
  }

  async function loadModule() {
    loading = true;
    error = "";
    try {
      const data = await invoke("library_load_module", { id: moduleId });
      moduleData = data;

      // .risum modules have structure: { type: "risuModule", module: { ... } }
      // or sometimes the module object directly
      const mod = data?.module ?? data;
      name = mod?.name ?? moduleName ?? "";
      description = mod?.description ?? "";

      // Deep-clone lorebook entries so mutations are local.
      // Preserve the original raw entry so unknown fields survive round-trip.
      const rawLore = mod?.lorebook ?? mod?.loreBook ?? [];
      lorebook = rawLore.map(entry => ({
        _raw: { ...entry },
        key: entry.key ?? (entry.keys ? entry.keys.join(", ") : ""),
        secondkey: entry.secondkey ?? entry.secondKey ?? "",
        content: entry.content ?? "",
        comment: entry.comment ?? "",
        insertorder: entry.insertorder ?? entry.insertOrder ?? 100,
        mode: entry.mode ?? "normal",
        alwaysActive: entry.alwaysActive ?? entry.always_active ?? false,
        selective: entry.selective ?? false,
        useRegex: entry.useRegex ?? entry.use_regex ?? false,
        activationPercent: entry.activationPercent ?? entry.activation_percent ?? 0,
        disable: entry.disable ?? false,
      }));

      const rawRegex = mod?.regex ?? [];
      regex = rawRegex.map(r => ({
        _raw: { ...r },
        comment: r.comment ?? "",
        in: r.in ?? r.pattern ?? "",
        out: r.out ?? "",
        type: r.type ?? r.script_type ?? "editinput",
        flag: r.flag ?? "",
        ableFlag: r.ableFlag ?? r.able_flag ?? true,
      }));
    } catch (e) {
      error = toUserError(e, "모듈 로드").message;
    }
    loading = false;
  }

  // Reconstruct module data with edits applied
  function buildModuleData() {
    const mod = moduleData?.module ?? moduleData ?? {};

    const updatedLorebook = lorebook.map(entry => ({
      ...(entry._raw || {}),
      key: entry.key,
      secondkey: entry.secondkey,
      content: entry.content,
      comment: entry.comment,
      insertorder: entry.insertorder,
      mode: entry.mode,
      alwaysActive: entry.alwaysActive,
      selective: entry.selective,
      useRegex: entry.useRegex,
      activationPercent: entry.activationPercent,
      disable: entry.disable,
      extentions: { ...(entry._raw?.extentions || {}), risu_case_sensitive: false },
    }));

    const updatedRegex = regex.map(r => ({
      ...(r._raw || {}),
      comment: r.comment,
      in: r.in,
      out: r.out,
      type: r.type,
      flag: r.flag,
      ableFlag: r.ableFlag,
    }));

    const updatedModule = {
      ...mod,
      name,
      description,
      lorebook: updatedLorebook,
      regex: updatedRegex,
    };

    // Preserve the outer wrapper if present
    if (moduleData?.type === "risuModule" || moduleData?.module) {
      return { ...moduleData, module: updatedModule };
    }
    return updatedModule;
  }

  async function saveModule() {
    saving = true;
    error = "";
    try {
      const data = buildModuleData();
      await invoke("cmd_save_module", { id: moduleId, name, data });
      flashStatus("저장 완료");
    } catch (e) {
      error = toUserError(e, "모듈 저장").message;
    }
    saving = false;
  }

  // Lorebook CRUD
  function addLoreEntry() {
    lorebook = [...lorebook, {
      key: "",
      secondkey: "",
      content: "",
      comment: "",
      insertorder: 100,
      mode: "normal",
      alwaysActive: false,
      selective: false,
      useRegex: false,
      activationPercent: 0,
      disable: false,
    }];
    expandedLore = lorebook.length - 1;
  }

  function removeLoreEntry(index) {
    lorebook = lorebook.filter((_, i) => i !== index);
    if (expandedLore === index) expandedLore = null;
    else if (expandedLore !== null && expandedLore > index) expandedLore--;
  }

  function toggleLoreExpand(index) {
    expandedLore = expandedLore === index ? null : index;
  }

  // Regex CRUD
  function addRegexEntry() {
    regex = [...regex, {
      comment: "",
      in: "",
      out: "",
      type: "editinput",
      flag: "",
      ableFlag: true,
    }];
    expandedRegex = regex.length - 1;
  }

  function removeRegexEntry(index) {
    regex = regex.filter((_, i) => i !== index);
    if (expandedRegex === index) expandedRegex = null;
    else if (expandedRegex !== null && expandedRegex > index) expandedRegex--;
  }

  function toggleRegexExpand(index) {
    expandedRegex = expandedRegex === index ? null : index;
  }

  function lorePreview(entry) {
    const key = entry.key || "(키 없음)";
    const content = entry.content || "";
    const preview = content.length > 60 ? content.substring(0, 60) + "..." : content;
    return { key, preview };
  }

  function regexPreview(entry) {
    const pattern = entry.in || "(패턴 없음)";
    const replacement = entry.out || "";
    const preview = replacement.length > 40 ? replacement.substring(0, 40) + "..." : replacement;
    return { pattern, preview };
  }

  $effect(() => {
    if (moduleId) loadModule();
  });
</script>

<div>
  <!-- Header bar -->
  <div style="display: flex; align-items: center; gap: 8px; margin-bottom: 12px;">
    <button class="btn btn-sm btn-secondary" onclick={onBack}>
      ← 뒤로
    </button>
    <span style="font-size: 16px; font-weight: 600; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
      {name || moduleName || "모듈 편집"}
    </span>
    <button class="btn btn-sm btn-primary" onclick={saveModule} disabled={saving || loading}>
      {saving ? "저장 중..." : "저장"}
    </button>
  </div>

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

  {#if loading}
    <div class="card">
      <div class="card-body" style="text-align: center;">
        <div class="spinner" style="margin: 0 auto;"></div>
      </div>
    </div>
  {:else}
    <!-- Module info -->
    <div class="card">
      <div class="card-header">
        <span class="card-title">모듈 정보</span>
      </div>
      <div class="card-body">
        <div class="field">
          <label class="label">이름</label>
          <input class="input" type="text" bind:value={name} placeholder="모듈 이름" />
        </div>
        <div class="field">
          <label class="label">설명</label>
          <ResizableTextarea bind:value={description} placeholder="모듈 설명" minHeight={60} />
        </div>
      </div>
    </div>

    <!-- Lorebook section -->
    <div class="card">
      <div class="card-header">
        <span class="card-title">로어북 ({lorebook.length})</span>
        <button class="btn btn-sm btn-primary" onclick={addLoreEntry}>추가</button>
      </div>

      {#if lorebook.length === 0}
        <div class="card-body" style="text-align: center; color: var(--fg3);">
          로어북 항목이 없습니다.
        </div>
      {:else}
        <ul class="prompt-list">
          {#each lorebook as entry, i (i)}
            <li class="prompt-item" style="padding: 0; flex-direction: column; align-items: stretch; cursor: default;">
              <!-- Summary row -->
              <button
                type="button"
                onclick={() => toggleLoreExpand(i)}
                style="display: flex; align-items: center; gap: 8px; width: 100%; padding: 10px 14px; background: transparent; border: 0; color: inherit; text-align: left; cursor: pointer; font: inherit;"
              >
                <span style="font-size: 13px; font-weight: 500; color: var(--fg); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                  {lorePreview(entry).key}
                </span>
                <span style="font-size: 11px; color: var(--fg3); max-width: 40%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                  {lorePreview(entry).preview}
                </span>
                <span style="font-size: 11px; color: {entry.disable ? 'var(--red)' : entry.alwaysActive ? 'var(--green)' : 'var(--fg3)'};">
                  {entry.disable ? "비활성" : entry.alwaysActive ? "항상 활성" : "조건"}
                </span>
                <span style="font-size: 14px; color: var(--fg3); flex-shrink: 0;">
                  {expandedLore === i ? "▲" : "▼"}
                </span>
              </button>

              <!-- Expanded editor -->
              {#if expandedLore === i}
                <div style="padding: 0 14px 14px; border-top: 1px solid var(--bg4);">
                  <div class="field" style="margin-top: 10px;">
                    <label class="label">키워드</label>
                    <input class="input" type="text" bind:value={entry.key} placeholder="트리거 키워드 (쉼표 구분)" />
                  </div>
                  <div class="field">
                    <label class="label">보조 키워드</label>
                    <input class="input" type="text" bind:value={entry.secondkey} placeholder="보조 키워드 (선택적 모드에서 사용)" />
                  </div>
                  <div class="field">
                    <label class="label">내용</label>
                    <ResizableTextarea bind:value={entry.content} placeholder="로어북 내용" minHeight={80} />
                  </div>
                  <div class="field">
                    <label class="label">코멘트</label>
                    <input class="input" type="text" bind:value={entry.comment} placeholder="메모 (선택)" />
                  </div>
                  <div style="display: flex; gap: 12px; align-items: center; flex-wrap: wrap;">
                    <div class="toggle-row" style="padding: 6px 0; gap: 6px;">
                      <span style="font-size: 12px; color: var(--fg2);">비활성화</span>
                      <label class="toggle">
                        <input type="checkbox" bind:checked={entry.disable} />
                        <span class="toggle-track"></span>
                      </label>
                    </div>
                    <div class="toggle-row" style="padding: 6px 0; gap: 6px;">
                      <span style="font-size: 12px; color: var(--fg2);">항상 활성</span>
                      <label class="toggle">
                        <input type="checkbox" bind:checked={entry.alwaysActive} />
                        <span class="toggle-track"></span>
                      </label>
                    </div>
                    <div class="toggle-row" style="padding: 6px 0; gap: 6px;">
                      <span style="font-size: 12px; color: var(--fg2);">선택적</span>
                      <label class="toggle">
                        <input type="checkbox" bind:checked={entry.selective} />
                        <span class="toggle-track"></span>
                      </label>
                    </div>
                  </div>
                  <div style="display: flex; gap: 12px; align-items: center; flex-wrap: wrap; margin-top: 6px;">
                    <div style="display: flex; align-items: center; gap: 6px; flex: 1; min-width: 180px;">
                      <label class="label" style="margin: 0; font-size: 12px; white-space: nowrap;">활성화 확률</label>
                      <input type="range" min="0" max="100" bind:value={entry.activationPercent} style="flex: 1; accent-color: var(--accent);" />
                      <span style="font-size: 11px; color: var(--fg3); min-width: 32px; text-align: right;">{entry.activationPercent}%</span>
                    </div>
                    <div style="display: flex; align-items: center; gap: 6px;">
                      <label class="label" style="margin: 0; font-size: 12px;">삽입 순서</label>
                      <input class="input" type="number" bind:value={entry.insertorder} style="width: 70px; padding: 4px 6px; font-size: 12px;" />
                    </div>
                  </div>
                  <div style="margin-top: 10px; display: flex; justify-content: flex-end;">
                    <button class="btn btn-sm btn-danger" onclick={() => removeLoreEntry(i)}>삭제</button>
                  </div>
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    <!-- Regex section -->
    <div class="card">
      <div class="card-header">
        <span class="card-title">정규식 ({regex.length})</span>
        <button class="btn btn-sm btn-primary" onclick={addRegexEntry}>추가</button>
      </div>

      {#if regex.length === 0}
        <div class="card-body" style="text-align: center; color: var(--fg3);">
          정규식 규칙이 없습니다.
        </div>
      {:else}
        <ul class="prompt-list">
          {#each regex as entry, i (i)}
            <li class="prompt-item" style="padding: 0; flex-direction: column; align-items: stretch; cursor: default;">
              <!-- Summary row -->
              <button
                type="button"
                onclick={() => toggleRegexExpand(i)}
                style="display: flex; align-items: center; gap: 8px; width: 100%; padding: 10px 14px; background: transparent; border: 0; color: inherit; text-align: left; cursor: pointer; font: inherit;"
              >
                <span style="font-size: 10px; font-weight: 600; text-transform: uppercase; padding: 2px 6px; border-radius: 4px; color: #fff; background: var(--type-plain); white-space: nowrap; flex-shrink: 0;">
                  {entry.type}
                </span>
                <span style="font-size: 13px; font-weight: 500; color: var(--fg); flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: monospace;">
                  {regexPreview(entry).pattern}
                </span>
                <span style="font-size: 11px; color: var(--fg3); max-width: 30%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                  → {regexPreview(entry).preview}
                </span>
                <span style="font-size: 14px; color: var(--fg3); flex-shrink: 0;">
                  {expandedRegex === i ? "▲" : "▼"}
                </span>
              </button>

              <!-- Expanded editor -->
              {#if expandedRegex === i}
                <div style="padding: 0 14px 14px; border-top: 1px solid var(--bg4);">
                  <div class="field" style="margin-top: 10px;">
                    <label class="label">코멘트</label>
                    <input class="input" type="text" bind:value={entry.comment} placeholder="규칙 설명 (선택)" />
                  </div>
                  <div class="field">
                    <label class="label">패턴 (IN)</label>
                    <ResizableTextarea bind:value={entry.in} placeholder="정규식 패턴" minHeight={60} style="font-family: monospace;" />
                  </div>
                  <div class="field">
                    <label class="label">치환 (OUT)</label>
                    <ResizableTextarea bind:value={entry.out} placeholder="치환 문자열" minHeight={60} style="font-family: monospace;" />
                  </div>
                  <div style="display: flex; gap: 10px; flex-wrap: wrap; align-items: flex-end;">
                    <div class="field" style="flex: 1; min-width: 120px; margin-bottom: 0;">
                      <label class="label">타입</label>
                      <select class="select" bind:value={entry.type} style="padding: 6px 8px; font-size: 13px;">
                        <option value="editinput">editinput</option>
                        <option value="editoutput">editoutput</option>
                        <option value="editdisplay">editdisplay</option>
                        <option value="edittrans">edittrans</option>
                      </select>
                    </div>
                    <div class="field" style="flex: 1; min-width: 100px; margin-bottom: 0;">
                      <label class="label">플래그</label>
                      <input class="input" type="text" bind:value={entry.flag} placeholder="g, i, m..." style="padding: 6px 8px; font-size: 13px;" />
                    </div>
                  </div>
                  <div style="margin-top: 10px; display: flex; justify-content: flex-end;">
                    <button class="btn btn-sm btn-danger" onclick={() => removeRegexEntry(i)}>삭제</button>
                  </div>
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    <!-- Bottom save button (mobile convenience) -->
    <div style="padding: 8px 0 16px;">
      <button class="btn btn-primary btn-block" onclick={saveModule} disabled={saving || loading}>
        {saving ? "저장 중..." : "저장"}
      </button>
    </div>
  {/if}
</div>
