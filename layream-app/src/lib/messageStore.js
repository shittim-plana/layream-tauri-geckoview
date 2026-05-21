export const MAIN_BRANCH_ID = "main";

// Stable id for each message -- used by HyPA Summary.chatMemos to link
// summaries back to the originating messages (and by hypa_pin_message /
// hypa_invalidate_summary cascades). crypto.randomUUID is available on
// every modern browser/WebView; fallback covers older Android WebViews.
export function newChatId() {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

/** Migrate legacy sessions: messages without parentId/branchId are treated
 *  as a linear "main" chain. Assigns parentId from array order and
 *  branchId = MAIN_BRANCH_ID. Returns { messages, branches, activeBranchId }. */
export function migrateSession(msgs, savedBranches, savedActiveBranchId) {
  if (!Array.isArray(msgs) || msgs.length === 0) {
    return {
      messages: [],
      branches: [{ id: MAIN_BRANCH_ID, name: "main", headId: null, forkPoint: null }],
      activeBranchId: MAIN_BRANCH_ID,
    };
  }

  // If first message already has branchId, session is already migrated
  if (msgs[0].branchId) {
    return {
      messages: msgs,
      branches: savedBranches && savedBranches.length > 0
        ? savedBranches
        : [{ id: MAIN_BRANCH_ID, name: "main", headId: msgs[msgs.length - 1].chatId, forkPoint: null }],
      activeBranchId: savedActiveBranchId || MAIN_BRANCH_ID,
    };
  }

  // Legacy migration: assign parentId chain and branchId
  const migrated = msgs.map((m, i) => ({
    ...m,
    parentId: i === 0 ? null : msgs[i - 1].chatId,
    branchId: MAIN_BRANCH_ID,
  }));

  const headId = migrated.length > 0 ? migrated[migrated.length - 1].chatId : null;
  return {
    messages: migrated,
    branches: [{ id: MAIN_BRANCH_ID, name: "main", headId, forkPoint: null }],
    activeBranchId: MAIN_BRANCH_ID,
  };
}

/** Build a lookup from chatId to message for O(1) access. */
export function buildIndex(msgs) {
  const idx = new Map();
  for (const m of msgs) idx.set(m.chatId, m);
  return idx;
}

/** Walk from a leaf message back to root via parentId chain.
 *  Returns messages in root-to-leaf order (conversation order). */
export function getChainToRoot(msgs, leafId) {
  const idx = buildIndex(msgs);
  const chain = [];
  let current = leafId;
  // Safety: max iterations to prevent infinite loops from corrupt data
  let safety = msgs.length + 1;
  while (current && safety-- > 0) {
    const msg = idx.get(current);
    if (!msg) break;
    chain.push(msg);
    current = msg.parentId;
  }
  chain.reverse();
  return chain;
}

/** Get the visible messages for the active branch.
 *  Pure function: messages array + branches + activeBranchId -> visible messages.
 *  Walks from the branch's head back to root via parentId. */
export function getVisibleMessages(msgs, branchList, activeId) {
  const branch = branchList.find(b => b.id === activeId);
  if (!branch || !branch.headId) return [];
  return getChainToRoot(msgs, branch.headId);
}

/** Count how many child branches fork from a given message chatId. */
export function countForks(branchList, chatId) {
  return branchList.filter(b => b.forkPoint === chatId).length;
}

/** Get branches that fork from a given message chatId. */
export function getBranchesAtForkPoint(branchList, chatId) {
  return branchList.filter(b => b.forkPoint === chatId);
}

/** Update the head of a branch. Returns new branches array. */
export function updateBranchHead(branchList, branchId, newHeadId) {
  return branchList.map(b =>
    b.id === branchId ? { ...b, headId: newHeadId } : b
  );
}

/** Create a new branch forking from forkPointId.
 *  Returns { newBranch, branches: updatedBranchesList }. */
export function createForkBranch(branchList, forkPointId, name) {
  const id = newChatId();
  const newBranch = { id, name, headId: forkPointId, forkPoint: forkPointId };
  return { newBranch, branches: [...branchList, newBranch] };
}

/** Append a message to the flat array and update the branch head.
 *  parentId is the current head of the branch. Returns { messages, branches, newMsg }. */
export function appendMessage(msgs, branchList, branchId, role, text, time, extraFields) {
  const branch = branchList.find(b => b.id === branchId);
  const parentId = branch?.headId || null;
  const chatId = newChatId();
  const newMsg = { chatId, parentId, branchId, role, text, time, ...(extraFields || {}) };
  const newMsgs = [...msgs, newMsg];
  const newBranches = updateBranchHead(branchList, branchId, chatId);
  return { messages: newMsgs, branches: newBranches, newMsg };
}
