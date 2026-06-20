<script>
  import { onMount } from "svelte";
  import { invoke } from "../lib/tauri.js";
  import { toUserError } from "../lib/errors.js";
  import { flashError } from "../lib/flashError.js";
  import { createAutosave } from "../lib/autosave.js";
  import ModuleEditView from "./ModuleEditView.svelte";
  import { getWorkspaceVersion } from "../lib/appStore.svelte.js";

  // --- Module edit state ---
  let editingModule = $state(null); // { id, name } | null

  function openModuleEdit(id, name) {
    editingModule = { id, name };
  }

  function closeModuleEdit() {
    editingModule = null;
    refresh("modules");
  }

  // --- Module activation state ---
  let enabledModules = $state([]);   // string[] of active module IDs

  async function loadEnabledModules() {
    try {
      const s = await invoke("cmd_load_settings") || {};
      enabledModules = Array.isArray(s.enabledModules) ? s.enabledModules : [];
    } catch (e) {
      console.warn("loadEnabledModules:", e);
      flashError(e, "활성 모듈 로드");
      enabledModules = [];
    }
  }

  const modulesAutosave = createAutosave(async () => {
    try {
      const existing = await invoke("cmd_load_settings") || {};
      await invoke("cmd_save_settings", {
        settings: { ...existing, enabledModules },
      });
    } catch (e) {
      error = toUserError(e, "모듈 설정 저장").message;
    }
  }, { delayMs: 500 });

  function scheduleModulesSave() {
    modulesAutosave.schedule();
  }

  function isModuleEnabled(moduleId) {
    return enabledModules.includes(moduleId);
  }

  function toggleModule(moduleId, event) {
    event.stopPropagation();
    if (enabledModules.includes(moduleId)) {
      enabledModules = enabledModules.filter(id => id !== moduleId);
    } else {
      enabledModules = [...enabledModules, moduleId];
    }
    scheduleModulesSave();
  }

  function enabledModuleCount() {
    return lists.modules.filter(m => enabledModules.includes(m.id)).length;
  }

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
    if (!k) { error = `Unknown kind: ${kindId}`; return; }
    try {
      const items = await invoke(k.list);
      lists[kindId] = Array.isArray(items) ? items : [];
    } catch (e) {
      error = toUserError(e, `${k.label} 목록 조회`).message;
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
    if (!k) { error = `Unknown kind: ${kindId}`; return; }
    if (!k.currentLoad) {
      error = `${k.label} have no current slot`;
      return;
    }
    try {
      const data = await invoke(k.currentLoad);
      if (!data || (typeof data === "object" && Object.keys(data).length === 0)) {
        error = `현재 로드된 ${k.label.toLowerCase().slice(0, -1)}이(가) 없습니다`;
        return;
      }
      const name = (data?.[k.currentNameField] && String(data[k.currentNameField]).trim()) || k.defaultName;
      await invoke(k.save, { name, data });
      flashStatus(`"${name}" 저장됨`);
      await refresh(kindId);
    } catch (e) {
      error = toUserError(e, "라이브러리 저장").message;
    }
  }

  async function loadItem(kindId, id) {
    const k = kindByID(kindId);
    if (!k) { error = `Unknown kind: ${kindId}`; return; }
    try {
      const data = await invoke(k.load, { id });
      if (k.currentSave && k.currentArgKey) {
        await invoke(k.currentSave, { [k.currentArgKey]: data });
        flashStatus(`불러옴 — ${k.label.slice(0, -1)} 탭에서 확인하세요`);
      } else {
        flashStatus(`불러옴 (${k.label}에 현재 슬롯 없음)`);
      }
    } catch (e) {
      error = toUserError(e, "라이브러리 로드").message;
    }
  }

  function askDelete(kindId, id, name) {
    confirmDelete = { kindId, id, name };
  }

  async function performDelete() {
    if (!confirmDelete) return;
    const { kindId, id } = confirmDelete;
    const k = kindByID(kindId);
    if (!k) { error = `Unknown kind: ${kindId}`; confirmDelete = null; return; }
    confirmDelete = null;
    try {
      await invoke(k.del, { id });
      flashStatus("삭제됨");
      await refresh(kindId);
    } catch (e) {
      error = toUserError(e, "삭제").message;
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
  let longPressFiredId = null;

  function startPress(kindId, item) {
    longPressFiredId = null;
    const key = `${kindId}:${item.id}`;
    clearTimeout(pressTimers.get(key));
    const t = setTimeout(() => {
      longPressFiredId = key;
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
    const key = `${kindId}:${item.id}`;
    if (longPressFiredId === key) {
      // The long-press already triggered the confirm dialog. Suppress the
      // click that fires on touchend so we don't load + delete in one gesture.
      longPressFiredId = null;
      return;
    }
    loadItem(kindId, item.id);
  }

  onMount(() => {
    refreshAll();
    loadEnabledModules();
  });

  // Re-load library data when workspace switches
  $effect(() => {
    const wsVersion = getWorkspaceVersion();
    if (wsVersion === 0) return;
    refreshAll();
    loadEnabledModules();
  });
</script>

{#if editingModule}
  <ModuleEditView
    moduleId={editingModule.id}
    moduleName={editingModule.name}
    onBack={closeModuleEdit}
  />
{:else}
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
          <span class="card-title">
            {k.label} ({lists[k.id].length})
            {#if k.id === "modules" && enabledModuleCount() > 0}
              <span style="font-size: 11px; color: var(--green); margin-left: 6px;">활성 {enabledModuleCount()}개</span>
            {/if}
          </span>
          <div style="display: flex; gap: 6px;">
            {#if k.currentLoad}
              <button class="btn btn-sm btn-primary" onclick={() => saveCurrent(k.id)}>
                현재 저장
              </button>
            {/if}
            <button class="btn btn-sm btn-secondary" onclick={() => refresh(k.id)}>
              새로고침
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
              저장된 {k.label.toLowerCase()}이(가) 없습니다. {k.label.slice(0, -1)} 탭에서 불러온 후 "현재 저장"을 클릭하세요.
            {:else}
              저장된 {k.label.toLowerCase()}이(가) 없습니다.
            {/if}
          </div>
        {:else}
          <ul class="prompt-list">
            {#each lists[k.id] as item (item.id)}
              <li class="prompt-item" style="padding: 0; {k.id === 'modules' && isModuleEnabled(item.id) ? 'border-left: 3px solid var(--green);' : ''}">
                <div style="display: flex; align-items: center; width: 100%;">
                  {#if k.id === "modules"}
                    <label
                      style="display: flex; align-items: center; padding: 10px 0 10px 12px; cursor: pointer; flex-shrink: 0;"
                      onclick={(e) => e.stopPropagation()}
                    >
                      <input
                        type="checkbox"
                        checked={isModuleEnabled(item.id)}
                        onchange={(e) => toggleModule(item.id, e)}
                        style="width: 16px; height: 16px; accent-color: var(--green); cursor: pointer;"
                      />
                    </label>
                  {/if}
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
                    style="display: flex; flex-direction: column; align-items: flex-start; gap: 2px; flex: 1; padding: 10px 14px; {k.id === 'modules' ? 'padding-left: 6px;' : ''} background: transparent; border: 0; color: inherit; text-align: left; cursor: pointer; font: inherit;"
                  >
                    <span style="font-size: 14px; color: var(--fg1); word-break: break-all;">
                      {item.name}
                      {#if k.id === "modules" && isModuleEnabled(item.id)}
                        <span style="font-size: 10px; color: var(--green); margin-left: 4px;">●</span>
                      {/if}
                    </span>
                    <span style="font-size: 11px; color: var(--fg3);">{formatTime(item.created_at)}</span>
                  </button>
                  {#if k.id === "modules"}
                    <button
                      type="button"
                      class="btn-icon"
                      onclick={(e) => { e.stopPropagation(); openModuleEdit(item.id, item.name); }}
                      title="편집"
                      style="padding: 8px 12px; flex-shrink: 0;"
                    >
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" style="width: 18px; height: 18px;">
                        <path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7" />
                        <path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z" />
                      </svg>
                    </button>
                  {/if}
                </div>
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
     
      style="position: fixed; inset: 0; background: rgba(0,0,0,0.5); display: flex; align-items: center; justify-content: center; z-index: 1000;"
    >
      <button
        type="button"
       
        onclick={() => confirmDelete = null}
        style="position: absolute; inset: 0; background: transparent; border: 0; cursor: default; padding: 0;"
      ></button>
      <div class="card" style="position: relative; max-width: 320px; margin: 16px;">
        <div class="card-header">
          <span class="card-title">삭제?</span>
        </div>
        <div class="card-body">
          <p style="margin-bottom: 12px;">{confirmDelete.name}</p>
          <div style="display: flex; gap: 8px; justify-content: flex-end;">
            <button class="btn btn-sm btn-secondary" onclick={() => confirmDelete = null}>취소</button>
            <button class="btn btn-sm btn-danger" onclick={performDelete}>삭제</button>
          </div>
        </div>
      </div>
    </div>
  {/if}
</div>
{/if}
