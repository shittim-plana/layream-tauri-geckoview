/**
 * Debounced autosave primitive.
 *
 * Wraps an async save function with schedule/flush/cancel controls.
 * - schedule(): resets the debounce timer and saves after delayMs.
 * - flush(): cancels any pending timer and saves immediately. Returns the save promise.
 * - cancel(): cancels any pending timer without saving.
 *
 * @param {() => Promise<void>|void} saveFn - The save function to debounce.
 * @param {{ delayMs?: number }} options
 * @returns {{ schedule: () => void, flush: () => Promise<void>|void, cancel: () => void }}
 */
export function createAutosave(saveFn, { delayMs = 1000 } = {}) {
  let timer = null;

  function schedule() {
    cancel();
    timer = setTimeout(() => { saveFn(); }, delayMs);
  }

  function flush() {
    cancel();
    return saveFn();
  }

  function cancel() {
    if (timer) { clearTimeout(timer); timer = null; }
  }

  return { schedule, flush, cancel };
}
