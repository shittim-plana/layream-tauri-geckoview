<script>
  import { isTauri } from "../lib/tauri.js";

  let { accept = "*/*", label = "파일 불러오기", extensions = "", onfile, disabled = false } = $props();

  let loading = $state(false);
  let dragover = $state(false);
  let debugMsg = $state("");
  let inputEl;

  async function pickFile() {
    if (disabled || loading) return;
    loading = true;
    debugMsg = "picking file...";

    try {
      if (isTauri()) {
        debugMsg = "trying tauri dialog...";
        try {
          const dialogMod = await import("@tauri-apps/plugin-dialog");
          debugMsg = "dialog imported, calling open()...";
          const selected = await dialogMod.open({ multiple: false });
          debugMsg = `open() returned: ${JSON.stringify(selected)?.slice(0, 200)}`;

          if (selected) {
            const path = typeof selected === "string" ? selected : selected.path || selected;
            debugMsg = `path: ${path}, reading file...`;

            const fsMod = await import("@tauri-apps/plugin-fs");
            const bytes = await fsMod.readFile(path);
            debugMsg = `read ${bytes.length} bytes`;

            const name = String(path).split("/").pop() || "file";
            onfile?.(name, Array.from(bytes));
            debugMsg = `sent ${bytes.length} bytes as "${name}"`;
          } else {
            debugMsg = "cancelled";
          }
        } catch (dialogErr) {
          debugMsg = `dialog error: ${dialogErr}, falling back to HTML input...`;
          await pickViaInput();
        }
      } else {
        debugMsg = "not tauri, using HTML input...";
        await pickViaInput();
      }
    } catch (e) {
      debugMsg = `error: ${e}`;
    }
    loading = false;
  }

  function pickViaInput() {
    return new Promise((resolve) => {
      if (!inputEl) { debugMsg = "no input element"; resolve(); return; }
      debugMsg = "opening HTML file input...";
      const handler = async (e) => {
        const file = e.target?.files?.[0];
        if (file) {
          debugMsg = `file selected: ${file.name} (${file.size} bytes), reading...`;
          try {
            const buf = await new Promise((res, rej) => {
              const reader = new FileReader();
              reader.onload = () => res(reader.result);
              reader.onerror = () => rej(reader.error);
              reader.readAsArrayBuffer(file);
            });
            debugMsg = `read ${buf.byteLength} bytes, converting...`;
            const data = Array.from(new Uint8Array(buf));
            debugMsg = `converted ${data.length} items, calling onfile...`;
            onfile?.(file.name, data);
            debugMsg = `done: ${file.name}`;
          } catch (err) {
            debugMsg = `read error: ${err}`;
          }
        } else {
          debugMsg = "no file in event";
        }
        if (inputEl) inputEl.value = "";
        resolve();
      };
      inputEl.addEventListener("change", handler, { once: true });
      inputEl.click();
      setTimeout(() => { debugMsg += " (timeout 30s)"; resolve(); }, 30000);
    });
  }

  function handleDrop(e) {
    e.preventDefault();
    dragover = false;
    const file = e.dataTransfer?.files?.[0];
    if (file && !loading) {
      loading = true;
      debugMsg = `dropped: ${file.name}`;
      (async () => {
        try {
          const buf = await new Promise((res, rej) => {
            const reader = new FileReader();
            reader.onload = () => res(reader.result);
            reader.onerror = () => rej(reader.error);
            reader.readAsArrayBuffer(file);
          });
          onfile?.(file.name, Array.from(new Uint8Array(buf)));
          debugMsg = `done: ${file.name}`;
        } catch (err) { debugMsg = `drop error: ${err}`; }
        loading = false;
      })();
    }
  }
</script>

<div
  class="drop-zone"
  class:dragover
  role="button"
  tabindex="0"
  onclick={pickFile}
  onkeydown={(e) => { if (e.key === "Enter") pickFile(); }}
  ondrop={handleDrop}
  ondragover={(e) => { e.preventDefault(); dragover = true; }}
  ondragleave={() => { dragover = false; }}
>
  {#if loading}
    <div class="spinner"></div>
    <p>Loading...</p>
  {:else}
    <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
      <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4" />
      <polyline points="17 8 12 3 7 8" />
      <line x1="12" y1="3" x2="12" y2="15" />
    </svg>
    <p>{label}</p>
    {#if extensions}
      <p style="font-size: 12px; margin-top: 4px;">{extensions}</p>
    {/if}
  {/if}
  {#if debugMsg}
    <p style="font-size: 10px; color: var(--orange); margin-top: 8px; word-break: break-all;">{debugMsg}</p>
  {/if}
  <input
    bind:this={inputEl}
    type="file"
    accept={accept}
    disabled={disabled || loading}
    style="display: none;"
  />
</div>
