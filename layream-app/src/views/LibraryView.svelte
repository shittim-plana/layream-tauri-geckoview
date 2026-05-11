<script>
  import { onMount } from "svelte";
  import { invoke } from "../lib/tauri.js";

  // Three kinds — same shape, different commands.
  // Keeping the kind metadata in one table so the loop body stays generic.
  const KINDS = [
    {
      id: "presets",
      label: "Presets",
      list: "library_list_presets",
      save: "library_save_preset",
      load: "library_load_preset",
      del:  "library_delete_preset",
      currentLoad: "cmd_load_current_preset",
      currentSave: "cmd_save_current_preset",
      currentNameField: "name",
      currentArgKey: "preset",
      defaultName: "Untitled Preset",
    },
    {
      id: "characters",
      label: "Characters",
      list: "library_list_characters",
      save: "library_save_character",
      load: "library_load_character",
      del:  "library_delete_character",
      currentLoad: "cmd_load_current_character",
      currentSave: "cmd_save_current_character",
      // Character is wrapped by CharacterView as { card, characterName, ... }.
      currentNameField: "characterName",
      currentArgKey: "character",
      defaultName: "Untitled Character",
    },
    {
      id: "modules",
      label: "Modules",
      list: "library_list_modules",
      save: "library_save_module",
      load: "library_load_module",
      del:  "library_delete_module",
      // Modules have no "current" slot in this app — only the library.
      currentLoad: null,
      currentSave: null,
      currentNameField: "name",
      currentArgKey: null,
      defaultName: "Untitled Module",
    },
  ];

  let activeKind = $state("presets");
  let lists = $state({ presets: [], characters: [], modules: [] });
  let loading = $state(false);
  let error = $state("");
  let status = $state("");
  let confirmDelete = $state(null); // { kindId, id, name } | null

  function kindByID(id) { return KINDS.find(k => k.id === id); }

  function flashStatus(msg) {
    status = msg;
    setTimeout(() => { if (status === msg) status = ""; }, 2000);
  }

  async function refresh(kindId) {
    const k = kindByID(kindId);
    try {
      const items = await invoke(k.list);
      lists[kindId] = Array.isArray(items) ? items : [];
    } catch (e) {
      error = `${k.label} list failed: ${String(e)}`;
    }
  }

  async function refreshAll() {
    loading = true;
    error = "";
    await Promise.all(KINDS.map(k => refresh(k.id)));
    loading = false;
  }

  async function saveCurrent(kindId) {
    const k = kindByID(kindId);
    if (!k.currentLoad) {
      error = `${k.label} have no current slot`;
      return;
    }
    try {
      const data = await invoke(k.currentLoad);
      if (!data || (typeof data === "object" && Object.keys(data).length === 0)) {
        error = `No current ${k.label.toLowerCase().slice(0, -1)} loaded`;
        return;
      }
      const name = (data?.[k.currentNameField] && String(data[k.currentNameField]).trim()) || k.defaultName;
      await invoke(k.save, { name, data });
      flashStatus(`Saved "${name}" to library`);
      await refresh(kindId);
    } catch (e) {
      error = `Save failed: ${String(e)}`;
    }
  }

  async function loadItem(kindId, id) {
    const k = kindByID(kindId);
    try {
      const data = await invoke(k.load, { id });
      if (k.currentSave && k.currentArgKey) {
        await invoke(k.currentSave, { [k.currentArgKey]: data });
        flashStatus(`Loaded — open ${k.label.slice(0, -1)} tab`);
      } else {
        flashStatus(`Loaded (no current slot for ${k.label})`);
      }
    } catch (e) {
      error = `Load failed: ${String(e)}`;
    }
  }

  function askDelete(kindId, id, name) {
    confirmDelete = { kindId, id, name };
  }

  async function performDelete() {
    if (!confirmDelete) return;
    const { kindId, id } = confirmDelete;
    const k = kindByID(kindId);
    confirmDelete = null;
    try {
      await invoke(k.del, { id });
      flashStatus("Deleted");
      await refresh(kindId);
    } catch (e) {
      error = `Delete failed: ${String(e)}`;
    }
  }

  function formatTime(secs) {
    if (!secs) return "";
    const d = new Date(secs * 1000);
    return d.toLocaleString();
  }

  // Long-press detection. 600ms is the conventional Android threshold —
  // short enough to feel responsive, long enough not to fire on a normal tap.
  // We track per-row state on the element itself via a Map keyed by id, so
  // multiple rows do not share a single timer (race when scrolling fast).
  const LONG_PRESS_MS = 600;
  const pressTimers = new Map();
  let longPressFired = false;

  function startPress(kindId, item) {
    longPressFired = false;
    const key = `${kindId}:${item.id}`;
    clearTimeout(pressTimers.get(key));
    const t = setTimeout(() => {
      longPressFired = true;
      askDelete(kindId, item.id, item.name);
    }, LONG_PRESS_MS);
    pressTimers.set(key, t);
  }

  function endPress(kindId, item) {
    const key = `${kindId}:${item.id}`;
    clearTimeout(pressTimers.get(key));
    pressTimers.delete(key);
  }

  function clickRow(kindId, item) {
    if (longPressFired) {
      // The long-press already triggered the confirm dialog. Suppress the
      // click that fires on touchend so we don't load + delete in one gesture.
      longPressFired = false;
      return;
    }
    loadItem(kindId, item.id);
  }

  onMount(refreshAll);
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

  <div class="tab-bar">
    {#each KINDS as k}
      <button class="tab-btn" class:active={activeKind === k.id} onclick={() => (activeKind = k.id)}>
        {k.label}
      </button>
    {/each}
  </div>

  {#each KINDS as k}
    <div style:display={activeKind === k.id ? "block" : "none"}>
      <div class="card">
        <div class="card-header">
          <span class="card-title">{k.label} ({lists[k.id].length})</span>
          <div style="display: flex; gap: 6px;">
            {#if k.currentLoad}
              <button class="btn btn-sm btn-primary" onclick={() => saveCurrent(k.id)}>
                Save current
              </button>
            {/if}
            <button class="btn btn-sm btn-secondary" onclick={() => refresh(k.id)}>
              Refresh
            </button>
          </div>
        </div>

        {#if loading}
          <div class="card-body" style="text-align: center;">
            <div class="spinner"></div>
          </div>
        {:else if lists[k.id].length === 0}
          <div class="card-body" style="text-align: center; color: var(--fg3);">
            {#if k.currentLoad}
              No saved {k.label.toLowerCase()}. Load one in the {k.label.slice(0, -1)} tab, then click "Save current".
            {:else}
              No saved {k.label.toLowerCase()}.
            {/if}
          </div>
        {:else}
          <ul class="prompt-list">
            {#each lists[k.id] as item (item.id)}
              <li class="prompt-item" style="padding: 0;">
                <button
                  type="button"
                  onclick={() => clickRow(k.id, item)}
                  ontouchstart={() => startPress(k.id, item)}
                  ontouchend={() => endPress(k.id, item)}
                  ontouchcancel={() => endPress(k.id, item)}
                  onmousedown={() => startPress(k.id, item)}
                  onmouseup={() => endPress(k.id, item)}
                  onmouseleave={() => endPress(k.id, item)}
                  oncontextmenu={(e) => { e.preventDefault(); askDelete(k.id, item.id, item.name); }}
                  onkeydown={(e) => { if (e.key === "Delete" || e.key === "Backspace") { e.preventDefault(); askDelete(k.id, item.id, item.name); } }}
                  style="display: flex; flex-direction: column; align-items: flex-start; gap: 2px; width: 100%; padding: 10px 14px; background: transparent; border: 0; color: inherit; text-align: left; cursor: pointer; font: inherit;"
                >
                  <span style="font-size: 14px; color: var(--fg1); word-break: break-all;">{item.name}</span>
                  <span style="font-size: 11px; color: var(--fg3);">{formatTime(item.created_at)}</span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    </div>
  {/each}

  {#if confirmDelete}
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Confirm delete"
      style="position: fixed; inset: 0; background: rgba(0,0,0,0.5); display: flex; align-items: center; justify-content: center; z-index: 1000;"
    >
      <button
        type="button"
        aria-label="Cancel"
        onclick={() => confirmDelete = null}
        style="position: absolute; inset: 0; background: transparent; border: 0; cursor: default; padding: 0;"
      ></button>
      <div class="card" style="position: relative; max-width: 320px; margin: 16px;">
        <div class="card-header">
          <span class="card-title">Delete?</span>
        </div>
        <div class="card-body">
          <p style="margin-bottom: 12px;">{confirmDelete.name}</p>
          <div style="display: flex; gap: 8px; justify-content: flex-end;">
            <button class="btn btn-sm btn-secondary" onclick={() => confirmDelete = null}>Cancel</button>
            <button class="btn btn-sm btn-danger" onclick={performDelete}>Delete</button>
          </div>
        </div>
      </div>
    </div>
  {/if}
</div>
