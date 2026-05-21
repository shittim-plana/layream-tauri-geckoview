<script>
  import { invoke } from "../lib/tauri.js";
  import { onMount } from "svelte";

  let workspaces = $state([]);
  let activeId = $state(null);
  let activeName = $state("기본");
  let open = $state(false);
  let loading = $state(false);
  let switching = $state(false);
  let newName = $state("");
  let showCreate = $state(false);
  let confirmDeleteId = $state(null);

  onMount(async () => {
    await loadSettings();
    await loadWorkspaces();
  });

  async function loadSettings() {
    try {
      const settings = await invoke("cmd_load_settings");
      if (settings?.active_workspace_id) {
        activeId = settings.active_workspace_id;
      }
    } catch (_) {}
  }

  async function loadWorkspaces() {
    loading = true;
    try {
      workspaces = (await invoke("cmd_workspace_list")) || [];
      // Resolve active workspace name
      if (activeId) {
        const active = workspaces.find((w) => w.id === activeId);
        activeName = active ? active.name : "기본";
        // If active workspace was deleted externally, reset
        if (!active) {
          activeId = null;
          await saveActiveId(null);
        }
      } else {
        activeName = "기본";
      }
    } catch (e) {
      console.error("Failed to load workspaces:", e);
    }
    loading = false;
  }

  async function saveActiveId(id) {
    try {
      const existing = (await invoke("cmd_load_settings")) || {};
      await invoke("cmd_save_settings", {
        settings: { ...existing, active_workspace_id: id },
      });
    } catch (e) {
      console.error("Failed to save active workspace:", e);
    }
  }

  async function saveCurrentSession(workspaceId) {
    if (!workspaceId) return;
    try {
      const session = await invoke("cmd_load_session");
      if (session) {
        await invoke("cmd_workspace_save_session_ws", {
          id: workspaceId,
          session,
        });
      }
    } catch (e) {
      console.error("Failed to save session for workspace:", e);
    }
  }

  async function loadWorkspaceSession(workspaceId) {
    if (!workspaceId) return;
    try {
      const session = await invoke("cmd_workspace_load_session_ws", {
        id: workspaceId,
      });
      if (session) {
        await invoke("cmd_save_session", { session });
      }
    } catch (e) {
      console.error("Failed to load workspace session:", e);
    }
  }

  async function switchWorkspace(id) {
    if (id === activeId || switching) return;
    switching = true;
    try {
      // 1. Save current session to current workspace (if any)
      if (activeId) {
        await saveCurrentSession(activeId);
      }
      // 2. Load new workspace's session into the main session slot
      if (id) {
        await loadWorkspaceSession(id);
      } else {
        // Switching to default — reset session to empty
        await invoke("cmd_save_session", { session: { messages: [] } });
      }
      // 3. Update active workspace in settings
      activeId = id;
      await saveActiveId(id);
      // 4. Refresh workspace list and resolve name
      await loadWorkspaces();
      // 5. Reload the page to pick up new session
      //    ChatView loads from cmd_load_session on mount, so a full reload
      //    is the simplest way to get all views in sync.
      open = false;
      window.location.reload();
    } catch (e) {
      console.error("Failed to switch workspace:", e);
    }
    switching = false;
  }

  async function createWorkspace() {
    const name = newName.trim();
    if (!name) return;
    try {
      const id = await invoke("cmd_workspace_create", { name });
      newName = "";
      showCreate = false;
      await loadWorkspaces();
      // Auto-switch to newly created workspace
      if (id) {
        await switchWorkspace(id);
      }
    } catch (e) {
      console.error("Failed to create workspace:", e);
    }
  }

  async function deleteWorkspace(id) {
    try {
      // Save current session before deleting the active workspace
      if (activeId === id) {
        await saveCurrentSession(id);
      }
      await invoke("cmd_workspace_delete", { id });
      confirmDeleteId = null;
      // If deleting the active workspace, switch to default
      if (activeId === id) {
        activeId = null;
        await saveActiveId(null);
        // Clear session to default empty
        await invoke("cmd_save_session", {
          session: { messages: [] },
        });
        window.location.reload();
        return;
      }
      await loadWorkspaces();
    } catch (e) {
      console.error("Failed to delete workspace:", e);
    }
  }

  function formatDate(ts) {
    if (!ts) return "";
    const d = new Date(ts * 1000);
    const y = d.getFullYear();
    const m = String(d.getMonth() + 1).padStart(2, "0");
    const day = String(d.getDate()).padStart(2, "0");
    return `${y}.${m}.${day}`;
  }

  function toggleDropdown() {
    open = !open;
    if (open) {
      loadWorkspaces();
      showCreate = false;
      confirmDeleteId = null;
    }
  }

  function handleKeydown(e) {
    if (e.key === "Enter") {
      createWorkspace();
    } else if (e.key === "Escape") {
      showCreate = false;
      newName = "";
    }
  }
</script>

<div class="ws-selector">
  <button class="ws-trigger" onclick={toggleDropdown} disabled={switching}>
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" width="16" height="16">
      <path d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
    </svg>
    <span class="ws-name">{switching ? "전환 중..." : activeName}</span>
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" width="12" height="12" class="ws-chevron" class:ws-chevron-open={open}>
      <path d="M6 9l6 6 6-6" />
    </svg>
  </button>

  {#if open}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="ws-backdrop" onclick={() => (open = false)} onkeydown={() => {}}></div>
    <div class="ws-dropdown">
      <div class="ws-dropdown-header">
        <span class="ws-dropdown-title">워크스페이스</span>
        <button
          class="ws-icon-btn"
          onclick={() => { showCreate = !showCreate; confirmDeleteId = null; }}
          title="새 워크스페이스"
        >
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" width="16" height="16">
            <path d="M12 5v14M5 12h14" />
          </svg>
        </button>
      </div>

      {#if showCreate}
        <div class="ws-create-row">
          <input
            class="ws-create-input"
            type="text"
            placeholder="워크스페이스 이름"
            bind:value={newName}
            onkeydown={handleKeydown}
          />
          <button
            class="ws-create-btn"
            onclick={createWorkspace}
            disabled={!newName.trim()}
          >
            생성
          </button>
        </div>
      {/if}

      <div class="ws-list">
        <!-- Default workspace -->
        <button
          class="ws-item"
          class:ws-item-active={!activeId}
          onclick={() => switchWorkspace(null)}
          disabled={switching}
        >
          <div class="ws-item-info">
            <span class="ws-item-name">기본</span>
            <span class="ws-item-meta">기본 워크스페이스</span>
          </div>
          {#if !activeId}
            <span class="ws-active-badge">활성</span>
          {/if}
        </button>

        {#each workspaces as ws (ws.id)}
          <div class="ws-item-wrap">
            <div
              class="ws-item"
              class:ws-item-active={activeId === ws.id}
              role="button"
              tabindex="0"
              onclick={() => { if (!switching) switchWorkspace(ws.id); }}
              onkeydown={(e) => { if (e.key === 'Enter' && !switching) switchWorkspace(ws.id); }}
            >
              <div class="ws-item-info">
                <span class="ws-item-name">{ws.name}</span>
                <span class="ws-item-meta">
                  {formatDate(ws.created_at)}
                  {#if ws.character_id}
                    &middot; 캐릭터
                  {/if}
                  {#if ws.preset_id}
                    &middot; 프리셋
                  {/if}
                </span>
              </div>
              <div class="ws-item-actions">
                {#if activeId === ws.id}
                  <span class="ws-active-badge">활성</span>
                {/if}
                <button
                  class="ws-delete-btn"
                  onclick={(e) => {
                    e.stopPropagation();
                    confirmDeleteId = confirmDeleteId === ws.id ? null : ws.id;
                  }}
                  title="삭제"
                >
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" width="14" height="14">
                    <path d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                  </svg>
                </button>
              </div>
            </div>
            {#if confirmDeleteId === ws.id}
              <div class="ws-confirm-delete">
                <span>"{ws.name}" 삭제?</span>
                <div class="ws-confirm-btns">
                  <button class="ws-confirm-yes" onclick={() => deleteWorkspace(ws.id)}>삭제</button>
                  <button class="ws-confirm-no" onclick={() => (confirmDeleteId = null)}>취소</button>
                </div>
              </div>
            {/if}
          </div>
        {/each}

        {#if workspaces.length === 0 && !loading}
          <div class="ws-empty">
            워크스페이스가 없습니다
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .ws-selector {
    position: relative;
  }

  .ws-trigger {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    background: var(--bg3);
    border: 1px solid var(--bg4);
    border-radius: var(--radius-sm);
    color: var(--fg);
    font-family: inherit;
    font-size: 12px;
    cursor: pointer;
    transition: border-color 0.15s;
    white-space: nowrap;
    max-width: 160px;
  }

  .ws-trigger:hover {
    border-color: var(--accent);
  }

  .ws-trigger:disabled {
    opacity: 0.6;
    cursor: wait;
  }

  .ws-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100px;
  }

  .ws-chevron {
    transition: transform 0.15s;
    flex-shrink: 0;
  }

  .ws-chevron-open {
    transform: rotate(180deg);
  }

  .ws-backdrop {
    position: fixed;
    inset: 0;
    z-index: 199;
  }

  .ws-dropdown {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    width: 280px;
    max-height: 400px;
    background: var(--bg2);
    border: 1px solid var(--bg4);
    border-radius: var(--radius);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
    z-index: 200;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    animation: ws-fadeIn 0.15s ease-out;
  }

  @keyframes ws-fadeIn {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .ws-dropdown-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    border-bottom: 1px solid var(--bg4);
  }

  .ws-dropdown-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--fg);
  }

  .ws-icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: none;
    background: none;
    color: var(--fg2);
    cursor: pointer;
    border-radius: 6px;
    transition: all 0.15s;
  }

  .ws-icon-btn:hover {
    background: var(--bg4);
    color: var(--accent);
  }

  .ws-create-row {
    display: flex;
    gap: 6px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--bg4);
  }

  .ws-create-input {
    flex: 1;
    background: var(--bg3);
    border: 1px solid var(--bg4);
    border-radius: 6px;
    color: var(--fg);
    font-family: inherit;
    font-size: 13px;
    padding: 6px 10px;
    outline: none;
  }

  .ws-create-input:focus {
    border-color: var(--accent);
  }

  .ws-create-btn {
    padding: 6px 12px;
    background: var(--accent);
    color: #fff;
    border: none;
    border-radius: 6px;
    font-family: inherit;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
  }

  .ws-create-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .ws-list {
    overflow-y: auto;
    flex: 1;
  }

  .ws-item-wrap {
    border-bottom: 1px solid var(--bg4);
  }

  .ws-item-wrap:last-child {
    border-bottom: none;
  }

  .ws-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    padding: 10px 12px;
    border: none;
    border-bottom: 1px solid var(--bg4);
    background: none;
    color: var(--fg);
    font-family: inherit;
    font-size: 13px;
    cursor: pointer;
    text-align: left;
    transition: background 0.1s;
  }

  .ws-item-wrap .ws-item {
    border-bottom: none;
  }

  .ws-item:hover {
    background: var(--bg3);
  }

  .ws-item:disabled {
    opacity: 0.5;
    cursor: wait;
  }

  .ws-item-active {
    background: var(--bg3);
  }

  .ws-item-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow: hidden;
    min-width: 0;
    flex: 1;
  }

  .ws-item-name {
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ws-item-meta {
    font-size: 11px;
    color: var(--fg3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ws-item-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-shrink: 0;
  }

  .ws-active-badge {
    font-size: 10px;
    font-weight: 600;
    color: var(--accent);
    padding: 2px 6px;
    background: rgba(124, 111, 255, 0.15);
    border-radius: 4px;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .ws-delete-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border: none;
    background: none;
    color: var(--fg3);
    cursor: pointer;
    border-radius: 4px;
    transition: all 0.15s;
  }

  .ws-delete-btn:hover {
    background: rgba(248, 113, 113, 0.15);
    color: var(--red);
  }

  .ws-confirm-delete {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    background: rgba(248, 113, 113, 0.08);
    font-size: 12px;
    color: var(--fg2);
  }

  .ws-confirm-btns {
    display: flex;
    gap: 6px;
  }

  .ws-confirm-yes {
    padding: 4px 10px;
    background: var(--red);
    color: #fff;
    border: none;
    border-radius: 4px;
    font-family: inherit;
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
  }

  .ws-confirm-no {
    padding: 4px 10px;
    background: var(--bg4);
    color: var(--fg);
    border: none;
    border-radius: 4px;
    font-family: inherit;
    font-size: 11px;
    cursor: pointer;
  }

  .ws-empty {
    padding: 24px;
    text-align: center;
    color: var(--fg3);
    font-size: 13px;
  }
</style>
