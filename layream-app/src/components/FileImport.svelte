<script>
  import { isTauri } from "../lib/tauri.js";

  let { accept = "*/*", label = "파일 불러오기", extensions = "", onfile, disabled = false } = $props();

  let loading = $state(false);
  let dragover = $state(false);
  let inputEl;

  async function pickFile() {
    if (disabled || loading) return;
    loading = true;
    try {
      if (isTauri) {
        const { open } = await import("@tauri-apps/plugin-dialog");
        const { readFile } = await import("@tauri-apps/plugin-fs");
        const selected = await open({ multiple: false });
        if (selected) {
          const path = typeof selected === "string" ? selected : selected.path;
          if (path) {
            const bytes = await readFile(path);
            const name = path.split("/").pop() || path.split("\\").pop() || "file";
            onfile?.(name, Array.from(bytes));
          }
        }
      } else {
        await pickViaInput();
      }
    } catch (e) {
      console.error("File pick error:", e);
      await pickViaInput();
    }
    loading = false;
  }

  function pickViaInput() {
    return new Promise((resolve) => {
      if (!inputEl) { resolve(); return; }
      inputEl.onchange = async (e) => {
        const file = e.target?.files?.[0];
        if (file) {
          try {
            const buf = await (file.arrayBuffer ? file.arrayBuffer() : readFileAsArrayBuffer(file));
            onfile?.(file.name, Array.from(new Uint8Array(buf)));
          } catch (err) { console.error("File read error:", err); }
        }
        inputEl.value = "";
        resolve();
      };
      inputEl.click();
      setTimeout(resolve, 30000);
    });
  }

  function readFileAsArrayBuffer(file) {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result);
      reader.onerror = () => reject(reader.error);
      reader.readAsArrayBuffer(file);
    });
  }

  function handleDrop(e) {
    e.preventDefault();
    dragover = false;
    const file = e.dataTransfer?.files?.[0];
    if (file && !loading) {
      loading = true;
      (async () => {
        try {
          const buf = await (file.arrayBuffer ? file.arrayBuffer() : readFileAsArrayBuffer(file));
          onfile?.(file.name, Array.from(new Uint8Array(buf)));
        } catch (err) { console.error("Drop read error:", err); }
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
  <input
    bind:this={inputEl}
    type="file"
    accept={accept}
    disabled={disabled || loading}
    style="display: none;"
  />
</div>
