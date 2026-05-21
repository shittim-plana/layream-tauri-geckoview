/**
 * Centralized toast/flash notification system for Layream.
 *
 * Provides a Svelte-compatible reactive store for toast messages.
 * Views import { flashError, flashWarning, flashStatus, toasts }
 * and the App shell renders the toast container.
 *
 * Design:
 * - Each toast has { id, message, kind, level, expires }.
 * - Toasts auto-clear after a configurable duration.
 * - Multiple toasts can be active simultaneously.
 * - Views that use inline error cards (PresetView, CharacterView, etc.)
 *   continue to manage their own `error` state — this module handles
 *   transient notifications only.
 */

import { ErrorKind, classifyError, userMessage } from './errors.js';

// Simple reactive store using Svelte 5 conventions.
// Callers read `toasts` (the array) and call flash* functions.
// Since Svelte 5 uses $state at the component level, we expose
// a module-level array + subscribe pattern via getter + mutation functions.

let _toasts = [];
let _listeners = [];
let _nextId = 1;

function notify() {
  for (const fn of _listeners) fn(_toasts);
}

/**
 * Subscribe to toast changes. Returns unsubscribe function.
 * Immediately calls the callback with the current value.
 *
 * @param {(toasts: Array) => void} fn
 * @returns {() => void} unsubscribe
 */
export function subscribe(fn) {
  _listeners.push(fn);
  fn(_toasts);
  return () => {
    _listeners = _listeners.filter(l => l !== fn);
  };
}

/**
 * Get the current toasts snapshot (for non-reactive reads).
 * @returns {Array}
 */
export function getToasts() {
  return _toasts;
}

const DEFAULT_ERROR_MS = 5000;
const DEFAULT_WARNING_MS = 4000;
const DEFAULT_STATUS_MS = 2500;

/**
 * Show an error toast. Classifies the raw error and shows a Korean message.
 *
 * @param {unknown} rawError — the error from a catch block
 * @param {string} [contextHint] — optional context like "프리셋 로드"
 * @param {number} [durationMs] — auto-clear after this many ms
 */
export function flashError(rawError, contextHint, durationMs = DEFAULT_ERROR_MS) {
  const kind = classifyError(rawError);

  // Cancelled errors are not worth showing to the user.
  if (kind === ErrorKind.CANCELLED) return;

  const message = userMessage(kind, contextHint);
  const id = _nextId++;
  const toast = { id, message, kind, level: 'error', detail: String(rawError) };
  _toasts = [..._toasts, toast];
  notify();

  setTimeout(() => { dismiss(id); }, durationMs);
}

/**
 * Show an error toast with a pre-composed message (no classification).
 * Use this when you already have a user-facing message string.
 *
 * @param {string} message
 * @param {number} [durationMs]
 */
export function flashErrorMessage(message, durationMs = DEFAULT_ERROR_MS) {
  const id = _nextId++;
  const toast = { id, message, kind: ErrorKind.UNKNOWN, level: 'error', detail: '' };
  _toasts = [..._toasts, toast];
  notify();

  setTimeout(() => { dismiss(id); }, durationMs);
}

/**
 * Show a warning toast.
 *
 * @param {string} message — user-facing warning text
 * @param {number} [durationMs]
 */
export function flashWarning(message, durationMs = DEFAULT_WARNING_MS) {
  const id = _nextId++;
  const toast = { id, message, kind: ErrorKind.UNKNOWN, level: 'warning', detail: '' };
  _toasts = [..._toasts, toast];
  notify();

  setTimeout(() => { dismiss(id); }, durationMs);
}

/**
 * Show a status/success toast.
 *
 * @param {string} message — user-facing status text
 * @param {number} [durationMs]
 */
export function flashStatus(message, durationMs = DEFAULT_STATUS_MS) {
  const id = _nextId++;
  const toast = { id, message, kind: ErrorKind.UNKNOWN, level: 'status', detail: '' };
  _toasts = [..._toasts, toast];
  notify();

  setTimeout(() => { dismiss(id); }, durationMs);
}

/**
 * Dismiss a specific toast by id.
 * @param {number} id
 */
export function dismiss(id) {
  const before = _toasts.length;
  _toasts = _toasts.filter(t => t.id !== id);
  if (_toasts.length !== before) notify();
}

/**
 * Dismiss all toasts.
 */
export function dismissAll() {
  if (_toasts.length === 0) return;
  _toasts = [];
  notify();
}
