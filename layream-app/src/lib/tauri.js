export function isTauri() {
  return "__TAURI__" in window || "__TAURI_INTERNALS__" in window;
}

export async function invoke(cmd, args) {
  if (isTauri()) {
    const { invoke: tauriInvoke } = await import("@tauri-apps/api/core");
    return tauriInvoke(cmd, args);
  }
  console.warn(`[mock] invoke: ${cmd}`, args);
  return null;
}
