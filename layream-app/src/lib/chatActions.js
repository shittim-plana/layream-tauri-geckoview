import { updateBranchHead } from "./messageStore.js";
import { runTriggers, createTriggerContext, TriggerType } from "./triggerEngine.js";

/**
 * Prepare state for regeneration: find trim point, collect alternatives,
 * compute messages/branches after removal.
 *
 * @param {object[]} visibleMessages - current visible message chain
 * @param {object[]} allMessages - full flat message array
 * @param {object[]} branches - branch metadata
 * @param {string} activeBranchId
 * @param {number} startIdx - index in visibleMessages to start trimming from
 *   (the target char message index for regenerateFrom, or last visible index for regenerateResponse)
 * @returns {{ savedAlts: string[], savedUserText: string, messages: object[], branches: object[] } | null}
 *   null if no valid user message found to regenerate from
 */
export function prepareRegeneration(visibleMessages, allMessages, branches, activeBranchId, startIdx) {
  const targetMsg = visibleMessages[startIdx];
  let savedAlts = [];
  if (targetMsg?.role === "char") {
    savedAlts = [...(targetMsg.alternatives || []), targetMsg.text];
  }

  let trimIdx = startIdx;
  while (trimIdx >= 0 && (visibleMessages[trimIdx].role === "char" || visibleMessages[trimIdx].role === "error")) {
    trimIdx--;
  }
  if (trimIdx < 0) return null;
  const lastUser = visibleMessages[trimIdx];
  if (lastUser.role !== "user") return null;

  const toRemove = new Set();
  for (let i = visibleMessages.length - 1; i >= trimIdx; i--) {
    toRemove.add(visibleMessages[i].chatId);
  }

  const newMessages = allMessages.filter(m => !toRemove.has(m.chatId));
  const newHead = trimIdx > 0 ? visibleMessages[trimIdx - 1].chatId : null;
  const newBranches = updateBranchHead(branches, activeBranchId, newHead);

  return {
    savedAlts,
    savedUserText: lastUser.text,
    messages: newMessages,
    branches: newBranches,
  };
}

/**
 * After regeneration, reattach saved alternatives to the new last char message.
 *
 * @param {object[]} messages - current flat messages
 * @param {object[]} updatedVisible - visible messages after regeneration
 * @param {string[]} savedAlts - saved alternative texts
 * @returns {object[]} updated messages array (or original if no change needed)
 */
export function reattachAlternatives(messages, updatedVisible, savedAlts) {
  if (savedAlts.length > 0 && updatedVisible.length > 0 && updatedVisible[updatedVisible.length - 1].role === "char") {
    const lastCharId = updatedVisible[updatedVisible.length - 1].chatId;
    return messages.map(m => m.chatId === lastCharId ? { ...m, alternatives: savedAlts } : m);
  }
  return messages;
}

/**
 * Apply context window trimming to a message array.
 *
 * @param {object[]} msgs - { role, text } array for the API
 * @param {number|null|undefined} maxContext - maxContext from preset
 * @param {number} systemLen - length of system prompt
 * @param {number} postLen - length of post-chat text
 * @returns {object[]} trimmed messages
 */
export function trimToContextWindow(msgs, maxContext, systemLen, postLen) {
  if (!maxContext || maxContext <= 0 || maxContext === -1000) return msgs;
  const maxChars = maxContext * 4;
  let total = systemLen + postLen;
  let keepFrom = 0;
  for (let i = msgs.length - 1; i >= 0; i--) {
    total += msgs[i].text?.length || 0;
    if (total > maxChars) { keepFrom = i + 1; break; }
  }
  return keepFrom > 0 ? msgs.slice(keepFrom) : msgs;
}

/**
 * Extract first_mes and alternate greetings from a character card.
 *
 * @param {object|null} loadedCharacter
 * @returns {string[]} array of greeting strings (empty if none)
 */
export function extractGreetings(loadedCharacter) {
  if (!loadedCharacter) return [];
  const card = loadedCharacter?.card?.data || loadedCharacter?.card || {};
  const firstMes = card.first_mes || "";
  const altGreetings = card.alternate_greetings || card.alternateGreetings || [];
  return [firstMes, ...altGreetings].filter(g => g && g.trim());
}

// ── Trigger script integration ───────────────────────────────────────

/**
 * Run `start` triggers when a new chat session begins.
 *
 * @param {object[]} triggerScripts - trigger scripts from character card
 * @param {Record<string, string>} triggerVars - session trigger variables
 * @returns {{ variables: Record<string, string> }}
 */
export function runStartTriggers(triggerScripts, triggerVars) {
  if (!Array.isArray(triggerScripts) || triggerScripts.length === 0) {
    return { variables: triggerVars || {} };
  }
  const ctx = createTriggerContext(triggerVars || {});
  runTriggers(triggerScripts, TriggerType.START, ctx);
  return { variables: ctx.variables };
}

/**
 * Run `input` triggers after user sends a message.
 *
 * @param {object[]} triggerScripts - trigger scripts from character card
 * @param {string} userText - the user's message
 * @param {Record<string, string>} triggerVars - session trigger variables
 * @param {object} [opts] - { chatIndex, recentMessages }
 * @returns {{ text: string, variables: Record<string, string> }}
 */
export function runInputTriggers(triggerScripts, userText, triggerVars, opts = {}) {
  if (!Array.isArray(triggerScripts) || triggerScripts.length === 0) {
    return { text: userText, variables: triggerVars || {} };
  }
  const ctx = createTriggerContext(triggerVars || {}, {
    text: userText,
    chatIndex: opts.chatIndex,
    recentMessages: opts.recentMessages,
  });
  runTriggers(triggerScripts, TriggerType.INPUT, ctx);
  return { text: ctx.text ?? userText, variables: ctx.variables };
}

/**
 * Run `output` triggers after AI responds.
 *
 * @param {object[]} triggerScripts - trigger scripts from character card
 * @param {string} aiText - the AI's response
 * @param {Record<string, string>} triggerVars - session trigger variables
 * @param {object} [opts] - { chatIndex, recentMessages }
 * @returns {{ text: string, variables: Record<string, string> }}
 */
export function runOutputTriggers(triggerScripts, aiText, triggerVars, opts = {}) {
  if (!Array.isArray(triggerScripts) || triggerScripts.length === 0) {
    return { text: aiText, variables: triggerVars || {} };
  }
  const ctx = createTriggerContext(triggerVars || {}, {
    text: aiText,
    chatIndex: opts.chatIndex,
    recentMessages: opts.recentMessages,
  });
  runTriggers(triggerScripts, TriggerType.OUTPUT, ctx);
  return { text: ctx.text ?? aiText, variables: ctx.variables };
}

/**
 * Run `chat_complete` triggers after a full exchange (user + AI).
 *
 * @param {object[]} triggerScripts - trigger scripts from character card
 * @param {Record<string, string>} triggerVars - session trigger variables
 * @param {object} [opts] - { chatIndex, recentMessages }
 * @returns {{ variables: Record<string, string> }}
 */
export function runChatCompleteTriggers(triggerScripts, triggerVars, opts = {}) {
  if (!Array.isArray(triggerScripts) || triggerScripts.length === 0) {
    return { variables: triggerVars || {} };
  }
  const ctx = createTriggerContext(triggerVars || {}, {
    chatIndex: opts.chatIndex,
    recentMessages: opts.recentMessages,
  });
  runTriggers(triggerScripts, TriggerType.CHAT_COMPLETE, ctx);
  return { variables: ctx.variables };
}
