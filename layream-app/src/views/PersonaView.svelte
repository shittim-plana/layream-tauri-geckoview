<script>
  import { onMount } from "svelte";
  import { invoke } from "../lib/tauri.js";
  import {
    getPersonas, getSelectedPersona,
    setPersonas, setSelectedPersona,
    savePersonas, loadPersonas,
    getCurrentCharacter,
  } from "../lib/appStore.svelte.js";

  let personas = $state([]);
  let selectedIdx = $state(-1);
  let editingIdx = $state(-1);
  let editName = $state("");
  let editPrompt = $state("");
  let editNote = $state("");
  let adding = $state(false);
  let newName = $state("");
  let newPrompt = $state("");

  function sync() {
    personas = getPersonas();
    selectedIdx = getSelectedPersona();
  }

  onMount(async () => {
    try {
      await loadPersonas(invoke);
    } catch (e) {
      console.warn("loadPersonas:", e);
    }
    sync();
  });

  function generateId() {
    return crypto.randomUUID ? crypto.randomUUID() : `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
  }

  async function persist(list, sel) {
    setPersonas(list);
    setSelectedPersona(sel);
    personas = list;
    selectedIdx = sel;
    try {
      await savePersonas(invoke, list, sel);
    } catch (e) {
      console.error("savePersonas:", e);
    }
  }

  async function selectPersona(idx) {
    const next = selectedIdx === idx ? -1 : idx;
    await persist(personas, next);
  }

  async function addPersona() {
    if (!newName.trim()) return;
    const p = {
      id: generateId(),
      name: newName.trim(),
      personaPrompt: newPrompt.trim(),
      icon: "",
      note: "",
    };
    const list = [...personas, p];
    adding = false;
    newName = "";
    newPrompt = "";
    await persist(list, selectedIdx);
  }

  function startEdit(idx) {
    editingIdx = idx;
    editName = personas[idx].name;
    editPrompt = personas[idx].personaPrompt || "";
    editNote = personas[idx].note || "";
  }

  async function saveEdit() {
    if (editingIdx < 0) return;
    const list = [...personas];
    list[editingIdx] = {
      ...list[editingIdx],
      name: editName.trim() || list[editingIdx].name,
      personaPrompt: editPrompt,
      note: editNote,
    };
    editingIdx = -1;
    await persist(list, selectedIdx);
  }

  function cancelEdit() {
    editingIdx = -1;
  }

  async function deletePersona(idx) {
    const list = personas.filter((_, i) => i !== idx);
    let sel = selectedIdx;
    if (sel === idx) sel = -1;
    else if (sel > idx) sel--;
    if (editingIdx === idx) editingIdx = -1;
    await persist(list, sel);
  }

  async function importFromCharacter() {
    const char = getCurrentCharacter();
    const card = char?.card?.data || char?.card || {};
    const personality = card.personality || "";
    const charName = card.name || "Character";
    if (!personality.trim()) return;
    const p = {
      id: generateId(),
      name: charName,
      personaPrompt: personality,
      icon: "",
      note: `${charName}에서 가져옴`,
    };
    const list = [...personas, p];
    await persist(list, selectedIdx);
  }
</script>

<div class="card">
  <div class="card-header">
    <span class="card-title">페르소나</span>
    <div style="display: flex; gap: 6px;">
      <button class="btn btn-sm btn-secondary" onclick={() => importFromCharacter()}>
        캐릭터에서 가져오기
      </button>
      <button class="btn btn-sm btn-primary" onclick={() => { adding = true; }}>
        추가
      </button>
    </div>
  </div>

  {#if adding}
    <div class="card-body" style="border-bottom: 1px solid var(--bg4);">
      <div class="field">
        <label class="label">이름</label>
        <input class="input" bind:value={newName} placeholder="페르소나 이름" />
      </div>
      <div class="field">
        <label class="label">프롬프트</label>
        <textarea class="textarea" bind:value={newPrompt} placeholder="페르소나 프롬프트 (CBS 지원)" rows="4"></textarea>
      </div>
      <div style="display: flex; gap: 8px; justify-content: flex-end;">
        <button class="btn btn-sm btn-secondary" onclick={() => { adding = false; newName = ""; newPrompt = ""; }}>취소</button>
        <button class="btn btn-sm btn-primary" onclick={addPersona}>추가</button>
      </div>
    </div>
  {/if}

  {#if personas.length === 0 && !adding}
    <div class="empty-state">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
        <path d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" />
      </svg>
      <p>페르소나가 없습니다.<br/>추가 버튼으로 새 페르소나를 만드세요.</p>
    </div>
  {/if}

  <ul class="prompt-list">
    {#each personas as persona, idx}
      <li
        class="prompt-item"
        class:active={selectedIdx === idx}
        onclick={() => selectPersona(idx)}
      >
        <div class="persona-avatar">
          {persona.icon || persona.name.charAt(0).toUpperCase()}
        </div>
        <div style="flex: 1; min-width: 0;">
          <div class="prompt-item-name">{persona.name}</div>
          {#if persona.note}
            <div class="prompt-item-text" style="font-size: 11px;">{persona.note}</div>
          {/if}
        </div>
        <div style="display: flex; gap: 4px;" onclick={(e) => e.stopPropagation()}>
          <button class="btn-icon" title="편집" onclick={() => startEdit(idx)}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
              <path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7" />
              <path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z" />
            </svg>
          </button>
          <button class="btn-icon" title="삭제" onclick={() => deletePersona(idx)}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
              <path d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
            </svg>
          </button>
        </div>
      </li>
    {/each}
  </ul>
</div>

{#if editingIdx >= 0}
  <div class="card" style="margin-top: 12px;">
    <div class="card-header">
      <span class="card-title">편집: {personas[editingIdx]?.name}</span>
    </div>
    <div class="card-body">
      <div class="field">
        <label class="label">이름</label>
        <input class="input" bind:value={editName} />
      </div>
      <div class="field">
        <label class="label">프롬프트</label>
        <textarea class="textarea" bind:value={editPrompt} rows="6"></textarea>
      </div>
      <div class="field">
        <label class="label">메모</label>
        <input class="input" bind:value={editNote} placeholder="메모 (선택)" />
      </div>
      <div style="display: flex; gap: 8px; justify-content: flex-end;">
        <button class="btn btn-sm btn-secondary" onclick={cancelEdit}>취소</button>
        <button class="btn btn-sm btn-primary" onclick={saveEdit}>저장</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .persona-avatar {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    background: var(--bg4);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 14px;
    font-weight: 600;
    color: var(--fg);
    flex-shrink: 0;
  }
</style>
