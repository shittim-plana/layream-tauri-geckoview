const isTauri = "__TAURI__" in window;

export async function invoke(cmd, args) {
  if (isTauri) {
    const { invoke: tauriInvoke } = await import("@tauri-apps/api/core");
    return tauriInvoke(cmd, args);
  }
  console.warn(`[mock] invoke: ${cmd}`, args);
  return null;
}

export function openFile(accept) {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    if (accept) input.accept = accept;
    input.onchange = () => resolve(input.files?.[0] || null);
    input.click();
  });
}

export { isTauri };
