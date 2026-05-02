<script>
  import { invoke } from "../lib/tauri.js";

  let character = $state(null);
  let loading = $state(false);
  let error = $state("");

  async function loadCharacter() {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".charx,.png,.json";
    input.onchange = async () => {
      const file = input.files?.[0];
      if (!file) return;
      loading = true;
      error = "";
      try {
        const buf = await file.arrayBuffer();
        const data = Array.from(new Uint8Array(buf));
        const result = await invoke("load_character", { name: file.name, data });
        if (result) {
          character = result;
        }
      } catch (e) {
        error = String(e);
      }
      loading = false;
    };
    input.click();
  }
</script>

<div>
  <div style="margin-bottom: 12px;">
    <button onclick={loadCharacter} disabled={loading}>
      {loading ? "Loading..." : "Load Character"}
    </button>
  </div>

  {#if error}
    <div class="card" style="border-color: var(--accent); color: var(--accent);">{error}</div>
  {/if}

  {#if character?.card}
    {@const card = character.card}
    <div class="card">
      <h2 style="font-size: 18px; margin-bottom: 8px;">{card.data?.name || card.name || "Unknown"}</h2>
      {#if card.data?.creator}
        <p style="font-size: 12px; color: var(--text-dim);">by {card.data.creator}</p>
      {/if}
    </div>

    {#if card.data?.description}
      <div class="card">
        <div class="field">
          <label>Description</label>
          <textarea rows="6" readonly value={card.data.description}></textarea>
        </div>
      </div>
    {/if}

    {#if card.data?.first_mes}
      <div class="card">
        <div class="field">
          <label>First Message</label>
          <textarea rows="4" readonly value={card.data.first_mes}></textarea>
        </div>
      </div>
    {/if}

    {#if card.data?.personality}
      <div class="card">
        <div class="field">
          <label>Personality</label>
          <textarea rows="3" readonly value={card.data.personality}></textarea>
        </div>
      </div>
    {/if}

    {#if card.data?.scenario}
      <div class="card">
        <div class="field">
          <label>Scenario</label>
          <textarea rows="3" readonly value={card.data.scenario}></textarea>
        </div>
      </div>
    {/if}

    {#if card.data?.character_book?.entries?.length}
      <div class="card">
        <label style="margin-bottom: 8px; display: block;">
          Lorebook ({card.data.character_book.entries.length} entries)
        </label>
        {#each card.data.character_book.entries as entry}
          <div style="padding: 4px 0; border-bottom: 1px solid var(--border); font-size: 13px;">
            <span style="color: var(--accent);">{entry.keys?.join(", ") || "?"}</span>
            <span style="color: var(--text-dim); margin-left: 8px;">
              {entry.content?.slice(0, 80)}{(entry.content?.length || 0) > 80 ? "..." : ""}
            </span>
          </div>
        {/each}
      </div>
    {/if}

    {#if character.assetCount > 0}
      <div class="card">
        <p style="color: var(--text-dim);">{character.assetCount} assets loaded</p>
      </div>
    {/if}
  {:else if !loading}
    <div class="card" style="text-align: center; padding: 48px;">
      <p style="color: var(--text-dim);">Load a character card to view</p>
      <p style="font-size: 12px; color: var(--text-dim); margin-top: 8px;">
        Supports .charx, .png, .json
      </p>
    </div>
  {/if}
</div>
