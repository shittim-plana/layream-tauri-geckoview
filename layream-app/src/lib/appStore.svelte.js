// Svelte 5 runes-based centralized store
// .svelte.js extension enables runes outside .svelte files

let _isSwitchingWorkspace = $state(false);
export function isSwitchingWorkspace() { return _isSwitchingWorkspace; }

let _settings = $state(null);
let _currentCharacter = $state(null);
let _currentPreset = $state(null);
let _settingsVersion = $state(0);
let _personas = $state([]);
let _selectedPersona = $state(-1);

// Active workspace ID — views observe this via $effect to re-initialize
// when the workspace changes without a full page reload.
let _activeWorkspaceId = $state(null);
let _workspaceVersion = $state(0);

export function getSettings() { return _settings; }
export function getCurrentCharacter() { return _currentCharacter; }
export function getCurrentPreset() { return _currentPreset; }
export function getSettingsVersion() { return _settingsVersion; }
export function getPersonas() { return _personas; }
export function getSelectedPersona() { return _selectedPersona; }
export function setPersonas(p) { _personas = p; }
export function setSelectedPersona(i) { _selectedPersona = i; }
export function getActiveWorkspaceId() { return _activeWorkspaceId; }
export function getWorkspaceVersion() { return _workspaceVersion; }

/** Set the initial workspace ID during app boot (no reactivity trigger). */
export function initActiveWorkspaceId(id) {
  _activeWorkspaceId = id;
}

/**
 * Switch workspace: save current session, load new workspace session,
 * persist the active_workspace_id in settings, and bump the workspace
 * version so that views re-initialize via $effect.
 *
 * @param {Function} invoke - Tauri invoke
 * @param {string|null} newId - workspace ID to switch to (null = default)
 */
export async function switchWorkspace(invoke, newId) {
  const oldId = _activeWorkspaceId;
  _isSwitchingWorkspace = true;

  // 0. Flush unsaved frontend state before reading the backend session.
  //    Views (ChatView, HypaView) listen for "app-flush" and persist pending changes.
  try {
    const { emit } = await import("@tauri-apps/api/event");
    await emit("app-flush");
    // Brief grace period for async flush listeners to settle.
    await new Promise(r => setTimeout(r, 200));
  } catch (_) { /* emit unavailable outside Tauri */ }

  // 1. Save current session to outgoing workspace
  if (oldId) {
    try {
      const session = await invoke("cmd_load_session");
      if (session) {
        await invoke("cmd_workspace_save_session_ws", { id: oldId, session });
      }
    } catch (e) {
      console.error("Failed to save session for workspace:", e);
    }
  }

  // 2. Load incoming workspace's session (or clear for default)
  if (newId) {
    try {
      const session = await invoke("cmd_workspace_load_session_ws", { id: newId });
      if (session) {
        await invoke("cmd_save_session", { session });
      }
    } catch (e) {
      console.error("Failed to load workspace session:", e);
    }
  } else {
    await invoke("cmd_save_session", { session: { messages: [] } });
  }

  // 3. Persist active_workspace_id in settings
  try {
    const existing = (await invoke("cmd_load_settings")) || {};
    await invoke("cmd_save_settings", {
      settings: { ...existing, active_workspace_id: newId },
    });
  } catch (e) {
    console.error("Failed to save active workspace:", e);
  }

  // 4. Update reactive state — triggers $effect in views
  _activeWorkspaceId = newId;
  _workspaceVersion++;

  // 5. Reload cached store data (settings, character, preset)
  await loadAll(invoke);

  _isSwitchingWorkspace = false;
}

export async function loadSettings(invoke) {
  _settings = await invoke("cmd_load_settings") || {};
  _settingsVersion++;
  return _settings;
}

export async function saveSettings(invoke, settings) {
  await invoke("cmd_save_settings", { settings });
  _settings = settings;
  _settingsVersion++;
}

export async function loadCurrentCharacter(invoke) {
  _currentCharacter = await invoke("cmd_load_current_character");
  return _currentCharacter;
}

export async function loadCurrentPreset(invoke) {
  _currentPreset = await invoke("cmd_load_current_preset");
  return _currentPreset;
}

export async function loadAll(invoke) {
  const [s, c, p, per] = await Promise.allSettled([
    invoke("cmd_load_settings"),
    invoke("cmd_load_current_character"),
    invoke("cmd_load_current_preset"),
    invoke("cmd_load_personas"),
  ]);
  if (s.status === "fulfilled") { _settings = s.value || {}; }
  if (c.status === "fulfilled") { _currentCharacter = c.value; }
  if (p.status === "fulfilled") { _currentPreset = p.value; }
  if (per.status === "fulfilled" && per.value) {
    _personas = per.value.personas || [];
    _selectedPersona = per.value.selectedPersona ?? -1;
  }
  _settingsVersion++;
  return { settings: _settings, character: _currentCharacter, preset: _currentPreset };
}

export async function loadPersonas(invoke) {
  const data = await invoke("cmd_load_personas");
  _personas = data?.personas || [];
  _selectedPersona = data?.selectedPersona ?? -1;
  return { personas: _personas, selectedPersona: _selectedPersona };
}

export async function savePersonas(invoke, personas, selectedPersona) {
  const data = { personas, selectedPersona };
  await invoke("cmd_save_personas", { personas: data });
  _personas = personas;
  _selectedPersona = selectedPersona;
}

/** Returns the personaPrompt of the currently selected persona, or null if none. */
export function getSelectedPersonaPrompt() {
  if (_selectedPersona < 0 || _selectedPersona >= _personas.length) return null;
  return _personas[_selectedPersona]?.personaPrompt || null;
}
