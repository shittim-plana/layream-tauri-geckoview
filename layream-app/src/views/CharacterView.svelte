<script>
  import { invoke } from "../lib/tauri.js";
  import FileImport from "../components/FileImport.svelte";

  let character = $state(null);
  let characterName = $state("");
  let loading = $state(false);
  let error = $state("");
  let subTab = $state("info");
  let moduleInfo = $state("");

  async function handleModuleFile(name, data) {
    moduleInfo = `${name} (${(data.length / 1024).toFixed(1)} KB)`;
  }

  async function handleFile(name, data) {
    loading = true;
    error = "";
    try {
      error = `invoking load_character: ${name} (${data.length} bytes)...`;
      const result = await invoke("load_character", { name, data });
      error = `result: ${result ? "ok" : "null"}, keys: ${result ? Object.keys(result).join(",") : "none"}`;
      if (result) {
        character = result;
        characterName = name;
        error = "";
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
  }

  function cardData() {
    return character?.card?.data || character?.card || null;
  }
</script>

<div>
  {#if error}
    <div class="card" style="border-color: var(--red); color: var(--red);">
      <div class="card-body">{error}</div>
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
        <button class="btn btn-sm btn-danger" onclick={closeCharacter}>Close</button>
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
        <div class="card-header"><span class="card-title">Assets</span></div>
        <div class="card-body">
          {#if character.assetCount > 0}
            <p style="color: var(--fg2);">{character.assetCount} assets embedded in character file.</p>
          {:else}
            <p style="color: var(--fg3);">No assets in this character file.</p>
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
          />
          {#if moduleInfo}
            <div style="margin-top: 8px; padding: 8px; background: var(--bg3); border-radius: var(--radius-sm); font-size: 12px; color: var(--fg2);">
              Module loaded: {moduleInfo}
            </div>
          {/if}
        </div>
      </div>
    {/if}
  {/if}
</div>
