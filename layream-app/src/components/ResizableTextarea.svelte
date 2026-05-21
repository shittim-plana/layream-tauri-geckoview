<script>
  /**
   * Wraps a textarea with a touch/mouse-draggable resize handle.
   *
   * Usage:
   *   <ResizableTextarea bind:value placeholder="..." />
   *
   * For non-bindable value patterns (e.g. value + oninput), pass both:
   *   <ResizableTextarea value={expr} oninput={(e) => ...} />
   *
   * Props:
   *   value      — textarea value (bindable)
   *   placeholder, readonly, rows, oninput, style — forwarded to <textarea>
   *   minHeight  — minimum height in px (default 100)
   *   className  — CSS class for the textarea (default "textarea")
   */
  let {
    value = $bindable(""),
    placeholder = "",
    readonly = false,
    minHeight = 100,
    className = "textarea",
    style = "",
    rows,
    oninput,
  } = $props();

  let height = $state(0);
  let dragStartY = 0;
  let dragStartH = 0;

  // Initialize height once from minHeight. The $effect ensures it reacts
  // if minHeight changes (e.g. when a caller computes it dynamically).
  $effect(() => {
    if (height === 0) height = minHeight;
  });

  function onDragStart(e) {
    const touch = e.touches?.[0] || e;
    dragStartY = touch.clientY;
    dragStartH = height;
    const onMove = (ev) => {
      const t = ev.touches?.[0] || ev;
      height = Math.max(minHeight, dragStartH + (t.clientY - dragStartY));
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
</script>

<div>
  <textarea
    class={className}
    bind:value
    {placeholder}
    {readonly}
    {rows}
    {oninput}
    style="height: {height}px; min-height: {minHeight}px; resize: none; {style}"
  ></textarea>
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="resize-handle"
    ontouchstart={onDragStart}
    onmousedown={onDragStart}
  >
    <span class="resize-grip"></span>
  </div>
</div>
