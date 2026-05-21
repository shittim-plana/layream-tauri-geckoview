/**
 * Centralized error classification for Layream frontend.
 *
 * Tauri commands surface Rust LayreamError as string-serialized messages.
 * This module classifies those strings into ErrorKind so callers can
 * branch on kind rather than fragile string matching at every catch site.
 */

export const ErrorKind = {
  NETWORK: 'network',
  AUTH: 'auth',
  PARSE: 'parse',
  STORAGE: 'storage',
  VALIDATION: 'validation',
  CANCELLED: 'cancelled',
  UNKNOWN: 'unknown',
};

/**
 * Classify an error (typically a string from Tauri invoke rejection)
 * into an ErrorKind value.
 *
 * @param {unknown} error
 * @returns {string} one of ErrorKind values
 */
export function classifyError(error) {
  const msg = String(error).toLowerCase();

  // Auth — token / OAuth / credential failures
  if (
    msg.includes('token') ||
    msg.includes('oauth') ||
    msg.includes('not connected') ||
    msg.includes('expired') ||
    msg.includes('unauthorized') ||
    msg.includes('401') ||
    msg.includes('credential') ||
    msg.includes('auth')
  ) {
    return ErrorKind.AUTH;
  }

  // Network — HTTP / connectivity failures
  if (
    msg.includes('network') ||
    msg.includes('http') ||
    msg.includes('timeout') ||
    msg.includes('timed out') ||
    msg.includes('connection') ||
    msg.includes('dns') ||
    msg.includes('fetch') ||
    msg.includes('502') ||
    msg.includes('503') ||
    msg.includes('504')
  ) {
    return ErrorKind.NETWORK;
  }

  // Parse — deserialization / format failures
  if (
    msg.includes('parse') ||
    msg.includes('json') ||
    msg.includes('invalid format') ||
    msg.includes('deserialize') ||
    msg.includes('decode') ||
    msg.includes('malformed') ||
    msg.includes('unexpected token') ||
    msg.includes('syntax')
  ) {
    return ErrorKind.PARSE;
  }

  // Storage — filesystem / persistence failures
  if (
    msg.includes('storage') ||
    msg.includes('permission denied') ||
    (msg.includes('read') && msg.includes('failed')) ||
    (msg.includes('write') && msg.includes('failed')) ||
    msg.includes('disk') ||
    msg.includes('io error') ||
    (msg.includes('not found') && (msg.includes('path') || msg.includes('directory')))
  ) {
    return ErrorKind.STORAGE;
  }

  // Validation — input / format constraints
  if (
    msg.includes('invalid') ||
    msg.includes('validation') ||
    msg.includes('지원하지 않는')
  ) {
    return ErrorKind.VALIDATION;
  }

  // Cancelled — user-initiated cancellation
  if (
    msg.includes('cancel') ||
    msg.includes('abort') ||
    msg.includes('취소')
  ) {
    return ErrorKind.CANCELLED;
  }

  return ErrorKind.UNKNOWN;
}

/**
 * Return a Korean user-facing message for the given ErrorKind.
 * `detail` is an optional extra string appended for context.
 *
 * @param {string} kind  — one of ErrorKind values
 * @param {string} [detail] — optional technical detail
 * @returns {string}
 */
export function userMessage(kind, detail) {
  const messages = {
    [ErrorKind.NETWORK]: '네트워크 오류가 발생했습니다. 인터넷 연결을 확인해 주세요.',
    [ErrorKind.AUTH]: '인증에 실패했습니다. Settings에서 연결 상태를 확인해 주세요.',
    [ErrorKind.PARSE]: '데이터 형식 오류입니다. 파일이 올바른 형식인지 확인해 주세요.',
    [ErrorKind.STORAGE]: '저장소 오류가 발생했습니다. 저장 공간과 권한을 확인해 주세요.',
    [ErrorKind.VALIDATION]: '입력값이 올바르지 않습니다.',
    [ErrorKind.CANCELLED]: '작업이 취소되었습니다.',
    [ErrorKind.UNKNOWN]: '알 수 없는 오류가 발생했습니다.',
  };

  const base = messages[kind] || messages[ErrorKind.UNKNOWN];
  if (detail) {
    return `${base} (${detail})`;
  }
  return base;
}

/**
 * Convenience: classify + produce user message in one call.
 *
 * @param {unknown} error — raw error from catch
 * @param {string} [contextHint] — optional context like "preset load"
 * @returns {{ kind: string, message: string }}
 */
export function toUserError(error, contextHint) {
  const kind = classifyError(error);
  const detail = contextHint || undefined;
  return { kind, message: userMessage(kind, detail) };
}
