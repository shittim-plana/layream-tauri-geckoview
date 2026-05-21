import { MAIN_BRANCH_ID, migrateSession } from "./messageStore.js";

/** Build the session payload object for persistence.
 *  @param {object} state - { messages, activeToggles, branches, activeBranchId, triggerVars } */
export function sessionPayload(state) {
  return {
    messages: state.messages,
    activeToggles: state.activeToggles,
    branches: state.branches,
    activeBranchId: state.activeBranchId,
    triggerVars: state.triggerVars || {},
  };
}

/** Load and migrate a persisted chat session.
 *  Returns { messages, branches, activeBranchId, activeToggles, triggerVars } or null on failure.
 *  @param {Function} invoke - Tauri invoke function
 *  @param {Function} flashError - error display callback */
export async function loadSession(invoke, flashError) {
  try {
    const savedSession = await invoke("cmd_load_session");
    if (savedSession?.messages?.length) {
      const migrated = migrateSession(
        savedSession.messages,
        savedSession.branches,
        savedSession.activeBranchId,
      );
      const activeToggles = (savedSession.activeToggles && typeof savedSession.activeToggles === "object")
        ? savedSession.activeToggles
        : {};
      const triggerVars = (savedSession.triggerVars && typeof savedSession.triggerVars === "object")
        ? savedSession.triggerVars
        : {};
      return { ...migrated, activeToggles, triggerVars };
    }
    if (savedSession?.activeToggles && typeof savedSession.activeToggles === "object") {
      const triggerVars = (savedSession.triggerVars && typeof savedSession.triggerVars === "object")
        ? savedSession.triggerVars
        : {};
      return { messages: null, branches: null, activeBranchId: null, activeToggles: savedSession.activeToggles, triggerVars };
    }
  } catch (e) {
    console.error("Failed to load session:", e);
    flashError(e, "이전 세션 로드");
  }
  return null;
}

/** Save session to disk via invoke. Debounced usage is caller's responsibility.
 *  @param {Function} invoke - Tauri invoke function
 *  @param {object} state - { messages, activeToggles, branches, activeBranchId, triggerVars } */
export async function saveSession(invoke, state) {
  await invoke("cmd_save_session", { session: sessionPayload(state) });
}

/** Clear the session: returns reset state and persists it.
 *  @param {Function} invoke - Tauri invoke function
 *  @param {Function} flashError - error display callback
 *  @param {object[]} toggleDefs - current toggle defs for resetting toggles
 *  @param {object} activeToggles - current toggles to reset */
export async function clearSession(invoke, flashError, toggleDefs, activeToggles) {
  const resetToggles = { ...activeToggles };
  for (const d of toggleDefs) resetToggles[d.key] = true;

  const state = {
    messages: [],
    branches: [{ id: MAIN_BRANCH_ID, name: "main", headId: null, forkPoint: null }],
    activeBranchId: MAIN_BRANCH_ID,
    activeToggles: resetToggles,
    triggerVars: {},
  };

  try {
    await invoke("cmd_save_session", { session: sessionPayload(state) });
  } catch (e) {
    console.error("session clear save failed:", e);
    flashError(e, "세션 초기화");
  }

  return state;
}
