<script>
  let { accept = "*/*", label = "파일 불러오기", extensions = "", onfile, disabled = false } = $props();

  let inputEl;
  let dragover = $state(false);
  let loading = $state(false);

  function handleClick() {
    if (!disabled && !loading && inputEl) inputEl.click();
  }

  async function processFile(file) {
    if (!file || !onfile) return;
    loading = true;
    try {
      let data;
      if (file.arrayBuffer) {
        const buf = await file.arrayBuffer();
        data = Array.from(new Uint8Array(buf));
      } else {
        data = await new Promise((resolve, reject) => {
          const reader = new FileReader();
          reader.onload = () => resolve(Array.from(new Uint8Array(reader.result)));
          reader.onerror = () => reject(reader.error);
          reader.readAsArrayBuffer(file);
        });
      }
      onfile(file.name, data);
    } catch (e) {
      console.error("File read error:", e);
    }
    loading = false;
  }

  function handleChange(e) {
    const file = e.target?.files?.[0];
    if (file) processFile(file);
    if (inputEl) inputEl.value = "";
  }

  function handleDrop(e) {
    e.preventDefault();
    dragover = false;
    const file = e.dataTransfer?.files?.[0];
    if (file) processFile(file);
  }

  function handleDragOver(e) {
    e.preventDefault();
    dragover = true;
  }

  function handleDragLeave() {
    dragover = false;
  }
</script>

<div
  class="drop-zone"
  class:dragover
  role="button"
  tabindex="0"
  onclick={handleClick}
  onkeydown={(e) => { if (e.key === "Enter") handleClick(); }}
  ondrop={handleDrop}
  ondragover={handleDragOver}
  ondragleave={handleDragLeave}
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
    onchange={handleChange}
    disabled={disabled || loading}
  />
</div>
