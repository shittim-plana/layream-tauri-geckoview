<script>
  import { onMount } from "svelte";
  import { invoke } from "../lib/tauri.js";
  import FileImport from "../components/FileImport.svelte";

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
      error = `Asset load failed: ${e}`;
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
      moduleError = `parse_risum error: ${String(e)}`;
    }
    moduleLoading = false;
  }

  async function handleFile(name, data, tempName) {
    const CHAR_EXTS = [".charx", ".png", ".jpeg", ".jpg", ".json"];
    const ext = "." + name.split(".").pop()?.toLowerCase();
    if (!CHAR_EXTS.includes(ext)) {
      error = `지원하지 않는 형식: ${ext} (${CHAR_EXTS.join(", ")} 만 가능)`;
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
        invoke("cmd_save_current_character", { character: { card: result.card, characterName: name, assetCount: result.assetCount, hasModule: result.hasModule } }).catch(e => console.warn("Auto-save:", e));
      } else {
        error = "load_character returned null/undefined";
      }
    } catch (e) {
      error = `load_character error: ${String(e)}`;
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
  }

  function cardData() {
    return character?.card?.data || character?.card || null;
  }

  let status = $state("");

  async function saveCharacter() {
    if (!character) return;
    try {
      await invoke("cmd_save_current_character", { character: { card: character.card, characterName, assetCount: character.assetCount, hasModule: character.hasModule } });
      status = "Saved!";
      setTimeout(() => { if (status === "Saved!") status = ""; }, 2000);
    } catch (e) {
      error = `Save failed: ${e}`;
    }
  }

  function formatBytes(bytes) {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  onMount(async () => {
    try {
      const saved = await invoke("cmd_load_current_character");
      if (saved && typeof saved === "object" && saved.card) {
        character = saved;
        characterName = saved.characterName || "Saved Character";
      }
    } catch (e) { console.warn("Load character failed:", e); }
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
          <button class="btn btn-sm btn-primary" onclick={saveCharacter}>Save</button>
          <button class="btn btn-sm btn-danger" onclick={closeCharacter}>Close</button>
        </div>
      </div>
      {#if data?.creator}
        <div class="card-body" style="padding: 8px 14px;">
          <span style="font-size: 12px; color: var(--fg2);">by {data.creator}</span>
          {#if character.assetCount > 0}
            <span style="font-size: 12px; color: var(--fg3); margin-left: 12px;">{character.assetCount} assets</span>
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
      {#if data.description}
        <div class="card">
          <div class="card-header"><span class="card-title">Description</span></div>
          <div class="card-body">
            <textarea class="textarea" rows="5" bind:value={data.description}></textarea>
          </div>
        </div>
      {/if}

      {#if data.personality}
        <div class="card">
          <div class="card-header"><span class="card-title">Personality</span></div>
          <div class="card-body">
            <textarea class="textarea" rows="3" bind:value={data.personality}></textarea>
          </div>
        </div>
      {/if}

      {#if data.scenario}
        <div class="card">
          <div class="card-header"><span class="card-title">Scenario</span></div>
          <div class="card-body">
            <textarea class="textarea" rows="3" bind:value={data.scenario}></textarea>
          </div>
        </div>
      {/if}

      {#if data.first_mes}
        <div class="card">
          <div class="card-header"><span class="card-title">First Message</span></div>
          <div class="card-body">
            <textarea class="textarea" rows="4" bind:value={data.first_mes}></textarea>
          </div>
        </div>
      {/if}

      <!-- Alternate Greetings -->
      {#if data.alternate_greetings?.length > 0}
        <div class="card">
          <div class="card-header">
            <span class="card-title">Alternate Greetings ({data.alternate_greetings.length})</span>
          </div>
          <div class="card-body">
            {#each data.alternate_greetings as greeting, i}
              <div style="padding: 8px 0; {i > 0 ? 'border-top: 1px solid var(--bg4);' : ''}">
                <label class="label" style="margin-bottom: 4px;">Greeting #{i + 1}</label>
                <textarea class="textarea" rows="3" bind:value={data.alternate_greetings[i]}></textarea>
              </div>
            {/each}
          </div>
        </div>
      {/if}

      {#if data.mes_example}
        <div class="card">
          <div class="card-header"><span class="card-title">Message Examples</span></div>
          <div class="card-body">
            <textarea class="textarea" rows="4" bind:value={data.mes_example}></textarea>
          </div>
        </div>
      {/if}

      {#if data.system_prompt}
        <div class="card">
          <div class="card-header"><span class="card-title">System Prompt</span></div>
          <div class="card-body">
            <textarea class="textarea" rows="3" bind:value={data.system_prompt}></textarea>
          </div>
        </div>
      {/if}

      <!-- Regex Scripts (from extensions.risuai.customScripts) -->
      {@const regexScripts = data.extensions?.risuai?.customScripts || []}
      {#if regexScripts.length > 0}
        <div class="card">
          <div class="card-header">
            <span class="card-title">Regex Scripts ({regexScripts.length})</span>
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
          <span class="card-title">Lorebook ({entries.length} entries)</span>
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
                  <textarea class="textarea" rows="3" bind:value={entry.content}></textarea>
                </div>
              </div>
            {/each}
          </div>
        {:else}
          <div class="card-body" style="text-align: center; color: var(--fg3);">
            No lorebook entries.
          </div>
        {/if}
      </div>
    {/if}

    <!-- Assets Tab -->
    {#if subTab === "assets"}
      <div class="card">
        <div class="card-header">
          <span class="card-title">Assets ({character.assetCount})</span>
        </div>
        <div class="card-body">
          {#if character.assetList?.length > 0}
            <div style="display: flex; flex-direction: column; gap: 4px;">
              {#each character.assetList as asset}
                <div
                  style="padding: 6px 8px; background: var(--bg3); border-radius: var(--radius-sm); {isImage(asset.name) ? 'cursor: pointer;' : ''}"
                  onclick={() => isImage(asset.name) && loadAssetPreview(asset.name)}
                >
                  <div style="display: flex; justify-content: space-between; align-items: center;">
                    <span style="font-size: 13px; color: {isImage(asset.name) ? 'var(--accent)' : 'var(--fg1)'}; word-break: break-all;">{asset.name}</span>
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
            <p style="color: var(--fg3); text-align: center;">No assets in this character file.</p>
          {/if}
        </div>
      </div>
    {/if}

    <!-- Module Tab -->
    {#if subTab === "module"}
      {#if character.hasModule}
        <div class="card">
          <div class="card-header"><span class="card-title">.risum Module (embedded)</span></div>
          <div class="card-body">
            <p style="color: var(--accent); font-size: 13px;">Module data detected in character file.</p>
          </div>
        </div>
      {/if}

      <div class="card">
        <div class="card-header"><span class="card-title">Load .risum Module</span></div>
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
              Module loaded: {moduleInfo}
            </div>
          {/if}
        </div>
      </div>

      {#if moduleParsed}
        <div class="card">
          <div class="card-header">
            <span class="card-title">{moduleParsed.name || "Unnamed Module"}</span>
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
