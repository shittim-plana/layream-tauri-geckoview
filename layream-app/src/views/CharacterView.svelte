<script>
  import { onMount } from "svelte";
  import { invoke } from "../lib/tauri.js";
  import { toUserError } from "../lib/errors.js";
  import FileImport from "../components/FileImport.svelte";
  import ResizableTextarea from "../components/ResizableTextarea.svelte";
  import { getWorkspaceVersion } from "../lib/appStore.svelte.js";

  let character = $state(null);
  let characterName = $state("");
  let loading = $state(false);
  let error = $state("");
  let subTab = $state("info");
  let moduleInfo = $state("");
  let moduleParsed = $state(null);
  let moduleError = $state("");
  let moduleLoading = $state(false);

  let previewAsset = $state(null);
  let previewData = $state("");
  let previewLoading = $state(false);

  // Greeting switcher: -1 selects data.first_mes; 0..N-1 selects
  // data.alternate_greetings[i]. Reset on character (re)load via $effect.
  let greetingIndex = $state(-1);

  // Edit mode toggle: default is view mode.
  let editMode = $state(false);

  const IMG_EXTS = [".png", ".jpg", ".jpeg", ".gif", ".webp"];
  function isImage(name) {
    return IMG_EXTS.some(ext => name.toLowerCase().endsWith(ext));
  }
  function mimeType(name) {
    const ext = name.split(".").pop()?.toLowerCase();
    if (ext === "jpg" || ext === "jpeg") return "image/jpeg";
    if (ext === "gif") return "image/gif";
    if (ext === "webp") return "image/webp";
    return "image/png";
  }

  async function loadAssetPreview(assetName) {
    if (previewAsset === assetName) { previewAsset = null; previewData = ""; return; }
    previewAsset = assetName;
    previewLoading = true;
    previewData = "";
    try {
      const b64 = await invoke("get_asset_data", { asset_name: assetName });
      previewData = `data:${mimeType(assetName)};base64,${b64}`;
    } catch (e) {
      previewData = "";
      previewAsset = null;
      error = toUserError(e, "에셋 로드").message;
    }
    previewLoading = false;
  }

  async function handleModuleFile(name, data, tempName) {
    moduleInfo = `${name} (${(data.length / 1024).toFixed(1)} KB)`;
    moduleError = "";
    moduleParsed = null;
    moduleLoading = true;
    try {
      const result = tempName
        ? await invoke("parse_risum_from_path", { temp_name: tempName })
        : await invoke("parse_risum", { data });
      moduleParsed = result;
    } catch (e) {
      moduleError = toUserError(e, "모듈 파싱").message;
    }
    moduleLoading = false;
  }

  async function handleFile(name, data, tempName) {
    const CHAR_EXTS = [".charx", ".png", ".jpeg", ".jpg", ".json"];
    const ext = "." + name.split(".").pop()?.toLowerCase();
    if (!CHAR_EXTS.includes(ext)) {
      error = `지원하지 않는 형식: ${ext} (${CHAR_EXTS.join(", ")}만 가능)`;
      return;
    }
    loading = true;
    error = "";
    try {
      const result = tempName
        ? await invoke("load_character_from_path", { name, temp_name: tempName })
        : await invoke("load_character", { name, data });
      if (result) {
        character = result;
        characterName = name;
        error = "";
        invoke("cmd_save_current_character", { character: { card: result.card, characterName: name, assetList: result.assetList, hasModule: result.hasModule } }).catch(e => console.error("Auto-save:", e));
      } else {
        error = "캐릭터를 로드할 수 없습니다. 파일 형식을 확인해 주세요.";
      }
    } catch (e) {
      error = toUserError(e, "캐릭터 로드").message;
    }
    loading = false;
  }

  function closeCharacter() {
    character = null;
    characterName = "";
    error = "";
    subTab = "info";
    moduleParsed = null;
    moduleInfo = "";
    moduleError = "";
    greetingIndex = -1;
    editMode = false;
  }

  // Total greeting count: first_mes counts as 1 if present, plus each
  // alternate_greetings entry. Used to bound the prev/next navigation.
  function greetingCount(d) {
    const altLen = d?.alternate_greetings?.length || 0;
    const hasFirst = typeof d?.first_mes === "string" && d.first_mes.length > 0;
    return (hasFirst ? 1 : 0) + altLen;
  }

  function getGreeting(d, idx) {
    if (idx < 0) return d?.first_mes ?? "";
    return d?.alternate_greetings?.[idx] ?? "";
  }

  function setGreeting(d, idx, value) {
    if (idx < 0) {
      d.first_mes = value;
    } else if (Array.isArray(d?.alternate_greetings)) {
      d.alternate_greetings[idx] = value;
    }
  }

  function addAlternateGreeting(d) {
    if (!d) return;
    if (!Array.isArray(d.alternate_greetings)) {
      d.alternate_greetings = [];
    }
    d.alternate_greetings = [...d.alternate_greetings, ""];
    greetingIndex = d.alternate_greetings.length - 1;
  }

  function removeAlternateGreeting(d, idx) {
    if (!d || !Array.isArray(d.alternate_greetings) || idx < 0) return;
    d.alternate_greetings = d.alternate_greetings.filter((_, i) => i !== idx);
    // Adjust greeting index after removal.
    const hasFirst = typeof d.first_mes === "string" && d.first_mes.length > 0;
    const altLen = d.alternate_greetings.length;
    if (altLen === 0) {
      greetingIndex = hasFirst ? -1 : -1;
    } else if (greetingIndex >= altLen) {
      greetingIndex = altLen - 1;
    }
  }

  function cardData() {
    return character?.card?.data || character?.card || null;
  }

  // Parse tags string to array.
  function tagsToString(tags) {
    if (!Array.isArray(tags)) return "";
    return tags.join(", ");
  }

  function stringToTags(str) {
    return str.split(",").map(t => t.trim()).filter(Boolean);
  }

  let status = $state("");

  async function saveCharacter() {
    if (!character) return;
    try {
      await invoke("cmd_save_current_character", { character: { card: character.card, characterName, assetList: character.assetList, hasModule: character.hasModule } });
      status = "저장 완료!";
      setTimeout(() => { if (status === "저장 완료!") status = ""; }, 2000);
    } catch (e) {
      error = toUserError(e, "캐릭터 저장").message;
    }
  }

  function formatBytes(bytes) {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  async function reloadCharacter() {
    try {
      const saved = await invoke("cmd_load_current_character");
      if (saved && typeof saved === "object" && saved.card) {
        character = saved;
        characterName = saved.characterName || "Saved Character";
      } else {
        character = null;
        characterName = "";
      }
    } catch (e) { console.error("Load character failed:", e); }
  }

  onMount(() => { reloadCharacter(); });

  // Re-load character when workspace switches
  $effect(() => {
    const wsVersion = getWorkspaceVersion();
    if (wsVersion === 0) return;
    error = "";
    reloadCharacter();
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

  {#if !character}
    <div class="empty-state animate-in">
      <FileImport
        accept="application/json,image/png,image/jpeg,application/octet-stream"
        label="캐릭터 파일 불러오기"
        extensions=".charx, .png, .jpeg, .json"
        onfile={handleFile}
        disabled={loading}
      />
      {#if loading}
        <div class="spinner"></div>
      {/if}
    </div>
  {:else}
    {@const data = cardData()}
    <!-- Header -->
    <div class="card">
      <div class="card-header">
        <span class="card-title">{data?.name || characterName}</span>
        <div style="display: flex; gap: 6px;">
          <button
            class="btn btn-sm btn-secondary"
            onclick={() => editMode = !editMode}
          >{editMode ? "보기" : "편집"}</button>
          {#if editMode}
            <button class="btn btn-sm btn-primary" onclick={saveCharacter}>저장</button>
          {/if}
          <button class="btn btn-sm btn-danger" onclick={closeCharacter}>Close</button>
        </div>
      </div>
      {#if data?.creator}
        <div class="card-body" style="padding: 8px 14px;">
          <span style="font-size: 12px; color: var(--fg2);">by {data.creator}</span>
          {#if character.assetList?.length > 0}
            <span style="font-size: 12px; color: var(--fg3); margin-left: 12px;">{character.assetList.length} assets</span>
          {/if}
          {#if character.hasModule}
            <span style="font-size: 12px; color: var(--accent); margin-left: 12px;">.risum module</span>
          {/if}
        </div>
      {/if}
    </div>

    <!-- Sub-tabs -->
    <div class="tab-bar">
      <button class="tab-btn" class:active={subTab === "info"} onclick={() => subTab = "info"}>Info</button>
      <button class="tab-btn" class:active={subTab === "lorebook"} onclick={() => subTab = "lorebook"}>Lorebook</button>
      <button class="tab-btn" class:active={subTab === "assets"} onclick={() => subTab = "assets"}>Assets</button>
      <button class="tab-btn" class:active={subTab === "module"} onclick={() => subTab = "module"}>Module</button>
    </div>

    <!-- Info Tab -->
    {#if subTab === "info" && data}

      <!-- Name -->
      <div class="card">
        <div class="card-header"><span class="card-title">이름</span></div>
        <div class="card-body">
          {#if editMode}
            <input class="input" type="text" bind:value={data.name} placeholder="캐릭터 이름" />
          {:else}
            <p style="font-size: 14px; color: var(--fg);">{data.name || "(없음)"}</p>
          {/if}
        </div>
      </div>

      <!-- Description -->
      <div class="card">
        <div class="card-header"><span class="card-title">설명 (Description)</span></div>
        <div class="card-body">
          {#if editMode}
            <ResizableTextarea bind:value={data.description} placeholder="캐릭터 설명" />
          {:else}
            {#if data.description}
              <pre style="font-size: 13px; color: var(--fg2); white-space: pre-wrap; word-wrap: break-word; font-family: inherit; line-height: 1.6;">{data.description}</pre>
            {:else}
              <p style="font-size: 13px; color: var(--fg3);">(없음)</p>
            {/if}
          {/if}
        </div>
      </div>

      <!-- Personality -->
      <div class="card">
        <div class="card-header"><span class="card-title">성격 (Personality)</span></div>
        <div class="card-body">
          {#if editMode}
            <ResizableTextarea bind:value={data.personality} placeholder="캐릭터 성격" />
          {:else}
            {#if data.personality}
              <pre style="font-size: 13px; color: var(--fg2); white-space: pre-wrap; word-wrap: break-word; font-family: inherit; line-height: 1.6;">{data.personality}</pre>
            {:else}
              <p style="font-size: 13px; color: var(--fg3);">(없음)</p>
            {/if}
          {/if}
        </div>
      </div>

      <!-- Scenario -->
      <div class="card">
        <div class="card-header"><span class="card-title">시나리오 (Scenario)</span></div>
        <div class="card-body">
          {#if editMode}
            <ResizableTextarea bind:value={data.scenario} placeholder="시나리오" />
          {:else}
            {#if data.scenario}
              <pre style="font-size: 13px; color: var(--fg2); white-space: pre-wrap; word-wrap: break-word; font-family: inherit; line-height: 1.6;">{data.scenario}</pre>
            {:else}
              <p style="font-size: 13px; color: var(--fg3);">(없음)</p>
            {/if}
          {/if}
        </div>
      </div>

      <!-- First Message & Alternate Greetings -->
      {@const greetTotal = greetingCount(data)}
      {@const altLen = data.alternate_greetings?.length || 0}
      {@const hasFirst = typeof data.first_mes === "string" && data.first_mes.length > 0}
      {#if editMode || greetTotal > 0}
        {@const minIdx = hasFirst ? -1 : (editMode ? -1 : 0)}
        {@const safeIdx = editMode
          ? Math.max(-1, Math.min(greetingIndex, altLen - 1))
          : Math.max(minIdx, Math.min(greetingIndex, altLen - 1))}
        {@const label = safeIdx < 0 ? "첫 메시지 (First Message)" : `대체 인사 #${safeIdx + 1} (Alternate)`}
        {@const position = safeIdx < 0 ? 1 : (hasFirst ? safeIdx + 2 : safeIdx + 1)}
        {@const totalDisplay = editMode ? (hasFirst || data.first_mes !== undefined ? 1 : 0) + altLen : greetTotal}
        <div class="card">
          <div class="card-header">
            <span class="card-title">{label}</span>
            <div style="display: flex; gap: 6px; align-items: center;">
              {#if totalDisplay > 1 || (!editMode && greetTotal > 1)}
                <button
                  class="btn btn-sm btn-secondary"
                  disabled={safeIdx <= (hasFirst ? -1 : 0)}
                  onclick={() => greetingIndex = safeIdx - 1}
                >&#8249;</button>
                <span style="font-size: 12px; color: var(--fg3);">{position}/{totalDisplay || greetTotal}</span>
                <button
                  class="btn btn-sm btn-secondary"
                  disabled={safeIdx >= altLen - 1}
                  onclick={() => greetingIndex = safeIdx + 1}
                >&#8250;</button>
              {/if}
              {#if editMode}
                <button
                  class="btn btn-sm btn-secondary"
                  onclick={() => addAlternateGreeting(data)}
                  title="대체 인사 추가"
                >+</button>
                {#if safeIdx >= 0}
                  <button
                    class="btn btn-sm btn-danger"
                    onclick={() => removeAlternateGreeting(data, safeIdx)}
                    title="이 대체 인사 삭제"
                  >-</button>
                {/if}
              {/if}
            </div>
          </div>
          <div class="card-body">
            {#if editMode}
              <ResizableTextarea
                value={getGreeting(data, safeIdx)}
                oninput={(e) => setGreeting(data, safeIdx, e.target.value)}
                placeholder={safeIdx < 0 ? "첫 메시지" : "대체 인사"}
              />
            {:else}
              {@const text = getGreeting(data, safeIdx)}
              {#if text}
                <pre style="font-size: 13px; color: var(--fg2); white-space: pre-wrap; word-wrap: break-word; font-family: inherit; line-height: 1.6;">{text}</pre>
              {:else}
                <p style="font-size: 13px; color: var(--fg3);">(없음)</p>
              {/if}
            {/if}
          </div>
        </div>
      {/if}

      <!-- Message Examples -->
      {#if editMode || data.mes_example}
        <div class="card">
          <div class="card-header"><span class="card-title">메시지 예시 (Message Examples)</span></div>
          <div class="card-body">
            {#if editMode}
              <ResizableTextarea bind:value={data.mes_example} placeholder="메시지 예시" />
            {:else}
              <pre style="font-size: 13px; color: var(--fg2); white-space: pre-wrap; word-wrap: break-word; font-family: inherit; line-height: 1.6;">{data.mes_example}</pre>
            {/if}
          </div>
        </div>
      {/if}

      <!-- System Prompt -->
      <div class="card">
        <div class="card-header"><span class="card-title">시스템 프롬프트 (System Prompt)</span></div>
        <div class="card-body">
          {#if editMode}
            <ResizableTextarea bind:value={data.system_prompt} placeholder="시스템 프롬프트" />
          {:else}
            {#if data.system_prompt}
              <pre style="font-size: 13px; color: var(--fg2); white-space: pre-wrap; word-wrap: break-word; font-family: inherit; line-height: 1.6;">{data.system_prompt}</pre>
            {:else}
              <p style="font-size: 13px; color: var(--fg3);">(없음)</p>
            {/if}
          {/if}
        </div>
      </div>

      <!-- Post History Instructions -->
      {#if editMode || data.post_history_instructions}
        <div class="card">
          <div class="card-header"><span class="card-title">후속 지시 (Post History Instructions)</span></div>
          <div class="card-body">
            {#if editMode}
              <ResizableTextarea bind:value={data.post_history_instructions} placeholder="후속 지시" />
            {:else}
              <pre style="font-size: 13px; color: var(--fg2); white-space: pre-wrap; word-wrap: break-word; font-family: inherit; line-height: 1.6;">{data.post_history_instructions}</pre>
            {/if}
          </div>
        </div>
      {/if}

      <!-- Creator Notes -->
      <div class="card">
        <div class="card-header"><span class="card-title">제작자 노트 (Creator Notes)</span></div>
        <div class="card-body">
          {#if editMode}
            <ResizableTextarea bind:value={data.creator_notes} placeholder="제작자 노트" />
          {:else}
            {#if data.creator_notes}
              <pre style="font-size: 13px; color: var(--fg2); white-space: pre-wrap; word-wrap: break-word; font-family: inherit; line-height: 1.6;">{data.creator_notes}</pre>
            {:else}
              <p style="font-size: 13px; color: var(--fg3);">(없음)</p>
            {/if}
          {/if}
        </div>
      </div>

      <!-- Tags -->
      <div class="card">
        <div class="card-header"><span class="card-title">태그 (Tags)</span></div>
        <div class="card-body">
          {#if editMode}
            <input
              class="input"
              type="text"
              value={tagsToString(data.tags)}
              oninput={(e) => { data.tags = stringToTags(e.target.value); }}
              placeholder="쉼표로 구분 (예: tag1, tag2, tag3)"
            />
          {:else}
            {#if data.tags?.length > 0}
              <div style="display: flex; flex-wrap: wrap; gap: 6px;">
                {#each data.tags as tag}
                  <span style="font-size: 12px; padding: 3px 8px; background: var(--bg4); border-radius: var(--radius-sm); color: var(--fg2);">{tag}</span>
                {/each}
              </div>
            {:else}
              <p style="font-size: 13px; color: var(--fg3);">(없음)</p>
            {/if}
          {/if}
        </div>
      </div>

      <!-- Regex Scripts (view only) -->
      {@const regexScripts = data.customScripts || data.extensions?.risuai?.customScripts || []}
      {#if regexScripts.length > 0}
        <div class="card">
          <div class="card-header">
            <span class="card-title">정규식 스크립트 ({regexScripts.length})</span>
          </div>
          <div class="card-body">
            {#each regexScripts as script, i}
              <div style="padding: 10px 0; {i > 0 ? 'border-top: 1px solid var(--bg4);' : ''}">
                <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 6px;">
                  <span style="color: var(--accent); font-size: 13px; font-weight: 500;">
                    {script.comment || `Script #${i + 1}`}
                  </span>
                  <span style="font-size: 11px; padding: 2px 6px; background: var(--bg4); border-radius: var(--radius-sm); color: var(--fg3);">
                    {script.type || "unknown"}
                  </span>
                </div>
                <div class="field">
                  <label class="label">Pattern (in)</label>
                  <input class="input" type="text" readonly value={script.in || ""} />
                </div>
                <div class="field">
                  <label class="label">Replacement (out)</label>
                  <input class="input" type="text" readonly value={script.out || ""} />
                </div>
              </div>
            {/each}
          </div>
        </div>
      {/if}
    {/if}

    <!-- Lorebook Tab -->
    {#if subTab === "lorebook"}
      {@const entries = data?.character_book?.entries || []}
      <div class="card">
        <div class="card-header">
          <span class="card-title">로어북 ({entries.length} entries)</span>
        </div>
        {#if entries.length > 0}
          <div class="card-body">
            {#each entries as entry, i}
              <div style="padding: 10px 0; border-bottom: 1px solid var(--bg4);">
                <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 6px;">
                  <span style="color: var(--accent); font-size: 13px; font-weight: 500;">
                    {entry.keys?.join(", ") || "?"}
                  </span>
                  <div style="display: flex; align-items: center; gap: 8px;">
                    {#if entry.enabled !== undefined}
                      <label class="toggle" style="transform: scale(0.8);">
                        <input type="checkbox" bind:checked={entry.enabled} />
                        <span class="toggle-track"></span>
                      </label>
                    {/if}
                  </div>
                </div>
                <div class="field">
                  <label class="label">Keywords</label>
                  <input class="input" type="text" value={entry.keys?.join(", ") || ""}
                    oninput={(e) => { entry.keys = e.target.value.split(",").map(k => k.trim()).filter(Boolean); }} />
                </div>
                <div class="field">
                  <label class="label">Content</label>
                  <ResizableTextarea bind:value={entry.content} />
                </div>
              </div>
            {/each}
          </div>
        {:else}
          <div class="card-body" style="text-align: center; color: var(--fg3);">
            로어북 항목이 없습니다.
          </div>
        {/if}
      </div>
    {/if}

    <!-- Assets Tab -->
    {#if subTab === "assets"}
      {@const additionalAssets = data?.extensions?.risuai?.additionalAssets || data?.additionalAssets || []}
      {@const assetNameMap = Object.fromEntries(additionalAssets.map(a => {
        const path = (a[1] || "").replace(/^__asset:/, "");
        return [path, a[0]];
      }))}
      <div class="card">
        <div class="card-header">
          <span class="card-title">에셋 ({character.assetList?.length ?? 0})</span>
        </div>
        <div class="card-body">
          {#if character.assetList?.length > 0}
            <div style="display: flex; flex-direction: column; gap: 4px;">
              {#each character.assetList as asset}
                {@const displayName = assetNameMap[asset.name] || asset.name.split('/').pop()}
                <div
                  style="padding: 6px 8px; background: var(--bg3); border-radius: var(--radius-sm); {isImage(asset.name) ? 'cursor: pointer;' : ''}"
                  onclick={() => isImage(asset.name) && loadAssetPreview(asset.name)}
                >
                  <div style="display: flex; justify-content: space-between; align-items: center;">
                    <span style="font-size: 13px; color: {isImage(asset.name) ? 'var(--accent)' : 'var(--fg1)'}; word-break: break-all;">{displayName}</span>
                    <span style="font-size: 12px; color: var(--fg3); white-space: nowrap; margin-left: 12px;">{formatBytes(asset.size)}</span>
                  </div>
                  {#if previewAsset === asset.name}
                    {#if previewLoading}
                      <div style="padding: 12px; text-align: center;"><div class="spinner"></div></div>
                    {:else if previewData}
                      <img src={previewData} alt={asset.name} style="max-width: 100%; margin-top: 8px; border-radius: var(--radius-sm);" />
                    {/if}
                  {/if}
                </div>
              {/each}
            </div>
          {:else}
            <p style="color: var(--fg3); text-align: center;">이 캐릭터 파일에 에셋이 없습니다.</p>
          {/if}
        </div>
      </div>
    {/if}

    <!-- Module Tab -->
    {#if subTab === "module"}
      {#if character.hasModule}
        <div class="card">
          <div class="card-header"><span class="card-title">.risum 모듈 (내장)</span></div>
          <div class="card-body">
            <p style="color: var(--accent); font-size: 13px;">캐릭터 파일에 모듈 데이터가 포함되어 있습니다.</p>
          </div>
        </div>
      {/if}

      <div class="card">
        <div class="card-header"><span class="card-title">.risum 모듈 불러오기</span></div>
        <div class="card-body">
          <FileImport
            accept="application/octet-stream,application/json"
            label=".risum 모듈 불러오기"
            extensions=".risum"
            onfile={handleModuleFile}
            disabled={moduleLoading}
          />
          {#if moduleLoading}
            <div class="spinner" style="margin-top: 8px;"></div>
          {/if}
          {#if moduleError}
            <div style="margin-top: 8px; padding: 8px; background: var(--bg3); border-radius: var(--radius-sm); font-size: 12px; color: var(--red);">
              {moduleError}
            </div>
          {/if}
          {#if moduleInfo && !moduleParsed && !moduleError && !moduleLoading}
            <div style="margin-top: 8px; padding: 8px; background: var(--bg3); border-radius: var(--radius-sm); font-size: 12px; color: var(--fg2);">
              모듈 로드됨: {moduleInfo}
            </div>
          {/if}
        </div>
      </div>

      {#if moduleParsed}
        <div class="card">
          <div class="card-header">
            <span class="card-title">{moduleParsed.name || "이름 없는 모듈"}</span>
          </div>
          <div class="card-body">
            {#if moduleParsed.description}
              <div class="field">
                <label class="label">Description</label>
                <p style="font-size: 13px; color: var(--fg2);">{moduleParsed.description}</p>
              </div>
            {/if}
            {#if moduleParsed.id}
              <div class="field">
                <label class="label">ID</label>
                <p style="font-size: 12px; color: var(--fg3); font-family: monospace;">{moduleParsed.id}</p>
              </div>
            {/if}
            {#if moduleParsed.namespace}
              <div class="field">
                <label class="label">Namespace</label>
                <p style="font-size: 12px; color: var(--fg3);">{moduleParsed.namespace}</p>
              </div>
            {/if}
            <div style="display: flex; gap: 12px; flex-wrap: wrap; margin-top: 8px;">
              {#if moduleParsed.lorebook?.length}
                <span style="font-size: 12px; padding: 2px 8px; background: var(--bg4); border-radius: var(--radius-sm); color: var(--fg2);">
                  {moduleParsed.lorebook.length} lorebook entries
                </span>
              {/if}
              {#if moduleParsed.regex?.length}
                <span style="font-size: 12px; padding: 2px 8px; background: var(--bg4); border-radius: var(--radius-sm); color: var(--fg2);">
                  {moduleParsed.regex.length} regex scripts
                </span>
              {/if}
              {#if moduleParsed.trigger?.length}
                <span style="font-size: 12px; padding: 2px 8px; background: var(--bg4); border-radius: var(--radius-sm); color: var(--fg2);">
                  {moduleParsed.trigger.length} trigger scripts
                </span>
              {/if}
              {#if moduleParsed.cjs}
                <span style="font-size: 12px; padding: 2px 8px; background: var(--bg4); border-radius: var(--radius-sm); color: var(--fg2);">
                  Custom JS
                </span>
              {/if}
              {#if moduleParsed.assets?.length}
                <span style="font-size: 12px; padding: 2px 8px; background: var(--bg4); border-radius: var(--radius-sm); color: var(--fg2);">
                  {moduleParsed.assets.length} assets
                </span>
              {/if}
            </div>
          </div>
        </div>

        <!-- Module Regex Scripts -->
        {#if moduleParsed.regex?.length > 0}
          <div class="card">
            <div class="card-header">
              <span class="card-title">Module Regex ({moduleParsed.regex.length})</span>
            </div>
            <div class="card-body">
              {#each moduleParsed.regex as script, i}
                <div style="padding: 10px 0; {i > 0 ? 'border-top: 1px solid var(--bg4);' : ''}">
                  <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 6px;">
                    <span style="color: var(--accent); font-size: 13px; font-weight: 500;">
                      {script.comment || `Script #${i + 1}`}
                    </span>
                    <span style="font-size: 11px; padding: 2px 6px; background: var(--bg4); border-radius: var(--radius-sm); color: var(--fg3);">
                      {script.type || "unknown"}
                    </span>
                  </div>
                  <div class="field">
                    <label class="label">Pattern (in)</label>
                    <input class="input" type="text" readonly value={script.in || ""} />
                  </div>
                  <div class="field">
                    <label class="label">Replacement (out)</label>
                    <input class="input" type="text" readonly value={script.out || ""} />
                  </div>
                </div>
              {/each}
            </div>
          </div>
        {/if}

        <!-- Module Lorebook -->
        {#if moduleParsed.lorebook?.length > 0}
          <div class="card">
            <div class="card-header">
              <span class="card-title">Module Lorebook ({moduleParsed.lorebook.length})</span>
            </div>
            <div class="card-body">
              {#each moduleParsed.lorebook as entry, i}
                <div style="padding: 10px 0; {i > 0 ? 'border-top: 1px solid var(--bg4);' : ''}">
                  <div style="margin-bottom: 4px;">
                    <span style="color: var(--accent); font-size: 13px; font-weight: 500;">
                      {entry.comment || entry.key || `Entry #${i + 1}`}
                    </span>
                  </div>
                  {#if entry.key}
                    <div class="field">
                      <label class="label">Key</label>
                      <input class="input" type="text" readonly value={entry.key} />
                    </div>
                  {/if}
                  {#if entry.content}
                    <div class="field">
                      <label class="label">Content</label>
                      <textarea class="textarea" rows="2" readonly value={entry.content}></textarea>
                    </div>
                  {/if}
                </div>
              {/each}
            </div>
          </div>
        {/if}
      {/if}
    {/if}
  {/if}
</div>
