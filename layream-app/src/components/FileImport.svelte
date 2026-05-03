<script>
  import { isTauri } from "../lib/tauri.js";
  import { writeFile, BaseDirectory } from "@tauri-apps/plugin-fs";

  let { accept = "*/*", label = "파일 불러오기", extensions = "", onfile, disabled = false } = $props();

  let loading = $state(false);
  let dragover = $state(false);
  let debugMsg = $state("");
  let loadingFile = $state("");
  let loadingSize = $state(0);
  let inputEl;

  const LARGE_FILE_MB = 50;
  const LARGE_FILE_THRESHOLD = 10 * 1024 * 1024;

  async function pickFile() {
    if (disabled || loading) return;
    loading = true;
    debugMsg = "";

    try {
      await pickViaInput();
    } catch (e) {
      debugMsg = `error: ${e}`;
    }
    loading = false;
  }

  function pickViaInput() {
    return new Promise((resolve) => {
      if (!inputEl) { debugMsg = "no input element"; resolve(); return; }
      let resolved = false;
      const handler = async (e) => {
        resolved = true;
        clearTimeout(tid);
        const file = e.target?.files?.[0];
        if (file) {
          loadingFile = file.name;
          loadingSize = file.size;
          const sizeMB = file.size / (1024 * 1024);
          if (sizeMB > LARGE_FILE_MB) {
            debugMsg = `⚠️ ${sizeMB.toFixed(0)}MB — 대용량 파일은 처리에 시간이 걸릴 수 있습니다`;
          }
          try {
            const buf = await new Promise((res, rej) => {
              const reader = new FileReader();
              reader.onload = () => res(reader.result);
              reader.onerror = () => rej(reader.error);
              reader.readAsArrayBuffer(file);
            });
            const data = new Uint8Array(buf);
            if (typeof onfile === "function") {
              try {
                if (file.size > LARGE_FILE_THRESHOLD) {
                  const tempName = `_import_${Date.now()}_${file.name.replace(/[^a-zA-Z0-9._-]/g, '_')}`;
                  await writeFile(tempName, data, { baseDir: BaseDirectory.AppData });
                  await onfile(file.name, data, tempName);
                } else {
                  await onfile(file.name, data);
                }
                debugMsg = "";
              } catch (invokeErr) {
                debugMsg = `error: ${invokeErr}`;
              }
            } else {
              debugMsg = `error: onfile is not a function (${typeof onfile})`;
            }
          } catch (err) {
            debugMsg = `read error: ${err}`;
          }
        }
        if (inputEl) inputEl.value = "";
        resolve();
      };
      inputEl.addEventListener("change", handler, { once: true });
      inputEl.click();
      const tid = setTimeout(() => {
        if (!resolved) {
          debugMsg = "cancelled";
          inputEl.removeEventListener("change", handler);
          resolve();
        }
      }, 120000);
    });
  }

  function handleDrop(e) {
    e.preventDefault();
    dragover = false;
    const file = e.dataTransfer?.files?.[0];
    if (file && !loading) {
      loading = true;
      debugMsg = "";
      (async () => {
        try {
          const buf = await new Promise((res, rej) => {
            const reader = new FileReader();
            reader.onload = () => res(reader.result);
            reader.onerror = () => rej(reader.error);
            reader.readAsArrayBuffer(file);
          });
          const dropData = new Uint8Array(buf);
          if (file.size > LARGE_FILE_THRESHOLD) {
            const tempName = `_import_${Date.now()}_${file.name.replace(/[^a-zA-Z0-9._-]/g, '_')}`;
            await writeFile(tempName, dropData, { baseDir: BaseDirectory.AppData });
            await onfile?.(file.name, dropData, tempName);
          } else {
            await onfile?.(file.name, dropData);
          }
        } catch (err) { debugMsg = `error: ${err}`; }
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
    {#if loadingFile}
      <p style="font-size: 13px; color: var(--fg2); margin-top: 8px; word-break: break-all;">{loadingFile}</p>
      <p style="font-size: 11px; color: var(--fg3);">{loadingSize < 1024*1024 ? (loadingSize/1024).toFixed(1) + ' KB' : (loadingSize/(1024*1024)).toFixed(1) + ' MB'}</p>
    {:else}
      <p>Loading...</p>
    {/if}
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
