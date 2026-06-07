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
  let highlightGen = 0;

  const DEPTH_CLASSES = ["cb0", "cb1", "cb2", "cb3", "cb4", "cb5"];

  function tokenClass(token) {
    if (token.kind === "control" || token.kind === "bracket") {
      const base = DEPTH_CLASSES[token.depth % DEPTH_CLASSES.length];
      return token.alt ? base + "a" : base;
    }
    if (token.kind === "macro") return "cbs-fn";
    if (token.kind === "variable") return "cbs-var";
    // Escape-region literals: plain styling, never CBS colors.
    if (token.kind === "escape") return "cbs-escape";
    // Markdown kinds arrive as "md-heading", "md-bold", … — map 1:1 to CSS classes.
    if (token.kind.startsWith("md-")) return "cbs-" + token.kind;
    return "";
  }

  function escapeHtml(str) {
    return str.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  }

  let lastTokens = [];
  let lastText = "";

  async function updateHighlight(text, forceRust = false) {
    const gen = ++highlightGen;

    // Incremental optimization: if the edit doesn't touch brackets,
    // shift existing token positions instead of a full Rust IPC call.
    if (!forceRust && lastTokens.length > 0 && lastText.length > 0) {
      const delta = text.length - lastText.length;
      // Find the first differing character position
      let diffStart = 0;
      const minLen = Math.min(text.length, lastText.length);
      while (diffStart < minLen && text[diffStart] === lastText[diffStart]) diffStart++;

      // Check if the changed region contains any bracket characters
      const changedRegion = delta >= 0
        ? text.slice(diffStart, diffStart + Math.abs(delta) + 1)
        : lastText.slice(diffStart, diffStart + Math.abs(delta) + 1);
      const hasBrackets = /[{}]/.test(changedRegion);

      if (!hasBrackets && delta !== 0) {
        // Shift token positions by delta for tokens after the edit point
        const shifted = lastTokens.map((t) => {
          if (t.start >= diffStart) {
            return { ...t, start: t.start + delta, end: t.end + delta };
          } else if (t.end > diffStart) {
            return { ...t, end: t.end + delta };
          }
          return t;
        }).filter((t) => t.start >= 0 && t.end > t.start && t.end <= text.length);

        lastTokens = shifted;
        lastText = text;
        highlightHtml = buildHtml(text, shifted);
        // Don't update diagnostics for incremental — they can wait for next full pass
        return;
      }
    }

    try {
      const result = await invoke("highlight_cbs", { input: text });
      if (!result || gen !== highlightGen) return;

      lastTokens = result.tokens || [];
      lastText = text;
      highlightHtml = buildHtml(text, result.tokens);
      diagnostics = result.diagnostics || [];
    } catch {
      if (gen !== highlightGen) return;
      lastTokens = [];
      lastText = text;
      highlightHtml = escapeHtml(text) + "\n";
      diagnostics = [];
    }
  }

  function buildHtml(text, tokens) {
    let html = "";
    let lastEnd = 0;
    for (const token of tokens) {
      if (token.start > lastEnd) {
        html += escapeHtml(text.slice(lastEnd, token.start));
      }
      const cls = tokenClass(token);
      html += `<span class="${cls}">${escapeHtml(text.slice(token.start, token.end))}</span>`;
      lastEnd = token.end;
    }
    if (lastEnd < text.length) {
      html += escapeHtml(text.slice(lastEnd));
    }
    return html + "\n";
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
    updateHighlight(value, true);
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
