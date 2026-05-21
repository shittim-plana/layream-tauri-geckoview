<script>
  import { invoke } from "../lib/tauri.js";

  let { value = "", onchange, readonly = false } = $props();

  let highlightHtml = $state("");
  let diagnostics = $state([]);
  let editorHeight = $state(200);
  let textareaEl;
  let highlightEl;
  let debounceTimer;
  let dragStartY = 0;
  let dragStartH = 0;

  const KIND_CLASS = {
    control: "cbs-block",
    macro: "cbs-fn",
    variable: "cbs-var",
    bracket: "cbs-identity",
  };

  function escapeHtml(str) {
    return str.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  }

  async function updateHighlight(text) {
    try {
      const result = await invoke("highlight_cbs", { input: text });
      if (!result) return;

      let html = "";
      let lastEnd = 0;
      for (const token of result.tokens) {
        if (token.start > lastEnd) {
          html += escapeHtml(text.slice(lastEnd, token.start));
        }
        const cls = KIND_CLASS[token.kind] || "";
        html += `<span class="${cls}">${escapeHtml(text.slice(token.start, token.end))}</span>`;
        lastEnd = token.end;
      }
      if (lastEnd < text.length) {
        html += escapeHtml(text.slice(lastEnd));
      }
      html += "\n";
      highlightHtml = html;
      diagnostics = result.diagnostics || [];
    } catch {
      highlightHtml = escapeHtml(text) + "\n";
      diagnostics = [];
    }
  }

  function handleInput(e) {
    const text = e.target.value;
    if (onchange) onchange(text);
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => updateHighlight(text), 80);
  }

  function syncScroll() {
    if (highlightEl && textareaEl) {
      highlightEl.scrollTop = textareaEl.scrollTop;
      highlightEl.scrollLeft = textareaEl.scrollLeft;
    }
  }

  function onDragStart(e) {
    const touch = e.touches?.[0] || e;
    dragStartY = touch.clientY;
    dragStartH = editorHeight;
    const onMove = (ev) => {
      const t = ev.touches?.[0] || ev;
      editorHeight = Math.max(80, dragStartH + (t.clientY - dragStartY));
      ev.preventDefault();
    };
    const onEnd = () => {
      window.removeEventListener("touchmove", onMove);
      window.removeEventListener("touchend", onEnd);
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onEnd);
    };
    window.addEventListener("touchmove", onMove, { passive: false });
    window.addEventListener("touchend", onEnd);
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onEnd);
  }

  $effect(() => {
    updateHighlight(value);
  });
</script>

<div class="editor-wrap" style="height: {editorHeight}px;">
  <div class="editor-highlight" bind:this={highlightEl}>
    {@html highlightHtml}
  </div>
  <textarea
    class="editor-textarea"
    bind:this={textareaEl}
    {value}
    oninput={handleInput}
    onscroll={syncScroll}
    {readonly}
    spellcheck="false"
    style="height: 100%; min-height: unset;"
  ></textarea>
</div>
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="resize-handle"
  ontouchstart={onDragStart}
  onmousedown={onDragStart}
>
  <span class="resize-grip"></span>
</div>
{#if diagnostics.length > 0}
  <div class="diagnostics">
    {#each diagnostics.slice(0, 5) as d}
      <div>Line {d.line}: {d.message}</div>
    {/each}
    {#if diagnostics.length > 5}
      <div style="color: var(--fg3);">...and {diagnostics.length - 5} more</div>
    {/if}
  </div>
{/if}
