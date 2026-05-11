<script>
  // HypaModal — view full summary text + metadata in a centered overlay.
  //
  // Props contract:
  //  - summary: object | null  (the SerializableHypaV3Data summary; null hides modal)
  //  - onClose: () => void     (parent closes by setting summary=null)
  //  - onToggleImportant: () => void  (parent flips isImportant + persists)
  //
  // Single-source-of-truth: parent owns the summary array. We only signal
  // intent — never mutate props directly.
  //
  // Reference: RisuAI HypaV3Modal (modal-summary-item.svelte) — full text textarea,
  // chatMemos count, isImportant star toggle. We omit translate/reroll/search
  // (out-of-scope for v0.3.0; can be layered later).

  let { summary, onClose, onToggleImportant } = $props();

  /** @type {HTMLButtonElement | undefined} */
  let closeBtnRef = $state();
  /** @type {HTMLDivElement | undefined} */
  let cardRef = $state();

  // --- Focus-on-open + body scroll lock ---
  // Detect summary null→non-null transition, focus the close button,
  // lock body scroll, and clean up on close.
  const FOCUSABLE_SELECTOR = 'a[href], button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

  $effect(() => {
    if (!summary) return;

    // Focus close button on next tick (after DOM mounts).
    requestAnimationFrame(() => closeBtnRef?.focus());

    // Lock body scroll while modal is open.
    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";

    return () => {
      document.body.style.overflow = prevOverflow;
    };
  });

  function previewChatMemos(memos) {
    if (!Array.isArray(memos) || memos.length === 0) return "(none)";
    if (memos.length <= 5) return memos.join(", ");
    return memos.slice(0, 5).join(", ") + ` … (+${memos.length - 5} more)`;
  }

  function fmtCreated(value) {
    if (!value) return null;
    const t = typeof value === "number" ? value : Date.parse(value);
    if (Number.isNaN(t)) return String(value);
    try {
      return new Date(t).toLocaleString();
    } catch {
      return String(value);
    }
  }

  function handleBackdropClick(e) {
    // Close only when clicking the backdrop, not the card.
    if (e.target === e.currentTarget) onClose?.();
  }

  function handleBackdropKey(e) {
    // Mirror click behavior: Enter/Space on backdrop also closes.
    if ((e.key === "Enter" || e.key === " ") && e.target === e.currentTarget) {
      e.preventDefault();
      onClose?.();
    }
  }

  function handleKeydown(e) {
    // Ignore keypresses when modal is closed.
    if (!summary) return;

    if (e.key === "Escape") {
      onClose?.();
      return;
    }

    // Focus trap: cycle Tab / Shift+Tab within the modal card.
    if (e.key === "Tab" && cardRef) {
      const focusable = /** @type {HTMLElement[]} */ (
        [...cardRef.querySelectorAll(FOCUSABLE_SELECTOR)]
      );
      if (focusable.length === 0) return;

      const first = focusable[0];
      const last = focusable[focusable.length - 1];

      if (e.shiftKey) {
        if (document.activeElement === first) {
          e.preventDefault();
          last.focus();
        }
      } else {
        if (document.activeElement === last) {
          e.preventDefault();
          first.focus();
        }
      }
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if summary}
  <div
    class="hypa-modal-backdrop"
    onclick={handleBackdropClick}
    onkeydown={handleBackdropKey}
    role="dialog"
    aria-modal="true"
    aria-label="Summary details"
    tabindex="-1"
  >
    <div class="hypa-modal-card" bind:this={cardRef}>
      <div class="hypa-modal-header">
        <span class="hypa-modal-title">Summary detail</span>
        <button class="btn btn-sm btn-secondary" bind:this={closeBtnRef} onclick={() => onClose?.()}>Close</button>
      </div>

      <div class="hypa-modal-body">
        <div class="hypa-meta-row">
          <span class="hypa-meta-label">Source messages</span>
          <span class="hypa-meta-value">{summary.chatMemos?.length ?? 0}</span>
        </div>

        <div class="hypa-meta-row">
          <span class="hypa-meta-label">isImportant</span>
          <label class="toggle">
            <input
              type="checkbox"
              checked={!!summary.isImportant}
              onchange={() => onToggleImportant?.()}
            />
            <span class="toggle-track"></span>
          </label>
        </div>

        <div class="hypa-meta-row">
          <span class="hypa-meta-label">pinBoost</span>
          <span class="hypa-meta-value">{summary.pinBoost ?? 0}</span>
        </div>

        <div class="hypa-meta-row">
          <span class="hypa-meta-label">Invalidated</span>
          <span class="hypa-meta-value" style:color={summary.invalidated ? "var(--red, #f55)" : "var(--fg2)"}>
            {summary.invalidated ? "yes" : "no"}
          </span>
        </div>

        {#if summary.embedding}
          <div class="hypa-meta-row">
            <span class="hypa-meta-label">Embedding</span>
            <span class="hypa-meta-value">{summary.embedding.length}-dim</span>
          </div>
        {/if}

        {#if fmtCreated(summary.createdAt ?? summary.created_at)}
          <div class="hypa-meta-row">
            <span class="hypa-meta-label">Created</span>
            <span class="hypa-meta-value">{fmtCreated(summary.createdAt ?? summary.created_at)}</span>
          </div>
        {/if}

        <div class="hypa-meta-row" style="flex-direction: column; align-items: stretch; gap: 4px;">
          <span class="hypa-meta-label">Connected message IDs</span>
          <span class="hypa-meta-value" style="font-family: monospace; font-size: 11px; word-break: break-all;">
            {previewChatMemos(summary.chatMemos)}
          </span>
        </div>

        <div class="hypa-meta-row" style="flex-direction: column; align-items: stretch; gap: 4px;">
          <span class="hypa-meta-label">Full text</span>
          <textarea
            class="textarea hypa-modal-text"
            readonly
            rows="12"
            value={summary.text ?? ""}
          ></textarea>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .hypa-modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 12px;
  }

  .hypa-modal-card {
    background: var(--bg2, #1a1a1a);
    border: 1px solid var(--bg4, #333);
    border-radius: 8px;
    width: 100%;
    max-width: 640px;
    max-height: calc(100dvh - 24px);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .hypa-modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--bg4, #333);
  }

  .hypa-modal-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--fg, #eee);
  }

  .hypa-modal-body {
    padding: 12px 14px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .hypa-meta-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .hypa-meta-label {
    font-size: 12px;
    color: var(--fg2, #999);
  }

  .hypa-meta-value {
    font-size: 13px;
    color: var(--fg, #eee);
  }

  .hypa-modal-text {
    width: 100%;
    resize: vertical;
    min-height: 160px;
    font-family: inherit;
  }
</style>
