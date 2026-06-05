<script>
  import { invoke, isTauri } from "../lib/tauri.js";
  import { onMount, onDestroy } from "svelte";
  import { toUserError, ErrorKind } from "../lib/errors.js";
  import { renderMarkdown } from "../lib/markdownRenderer.js";
  import ResizableTextarea from "../components/ResizableTextarea.svelte";
  import {
    MAIN_BRANCH_ID, appendMessage, getVisibleMessages,
    countForks, getBranchesAtForkPoint, createForkBranch,
  } from "../lib/messageStore.js";
  import { assemblePrompt, embedQueryForRag } from "../lib/assemblePrompt.js";
  import { loadSession, saveSession, clearSession } from "../lib/chatSession.js";
  import { startPolling, stopPolling, dispatchChat } from "../lib/streamingManager.js";
  import { prepareRegeneration, reattachAlternatives, trimToContextWindow, extractGreetings } from "../lib/chatActions.js";
  import { createAutosave } from "../lib/autosave.js";
  import { getWorkspaceVersion } from "../lib/appStore.svelte.js";

  let { onReady, hypaApi } = $props();

  // ── Reactive state ─────────────────────────────────────────────────
  let messages = $state([]);
  let chatInput = $state("");
  let streaming = $state(false);
  let sessionLoaded = $state(false);
  let streamingText = $state("");
  let greetings = $state([]);
  let greetingIndex = $state(0);
  let chatBottom;
  let unlisten;
  let unlistenAppFlush;
  let editingMsgId = $state(null);
  let editingText = $state("");
  let error = $state("");
  let chatProvider = $state("");
  let chatInputEl;
  let branches = $state([{ id: MAIN_BRANCH_ID, name: "main", headId: null, forkPoint: null }]);
  let activeBranchId = $state(MAIN_BRANCH_ID);
  let forkDropdownId = $state(null);
  let activeToggles = $state({});
  let toggleDefs = $state([]);
  let togglePanelOpen = $state(false);
  let charAvatarUrl = $state("");
  let charName = $state("");
  let chatMessagesEl = $state(null);
  let isNearBottom = $state(true);
  let copiedMsgId = $state(null);
  const STICK_TO_BOTTOM_PX = 120;
  const ERROR_CLEAR_MS = 3000;

  function handleMessagesScroll(e) {
    const el = e.target;
    isNearBottom = (el.scrollHeight - el.scrollTop - el.clientHeight) < STICK_TO_BOTTOM_PX;
  }
  function scrollToBottom() {
    isNearBottom = true;
    chatBottom?.scrollIntoView({ block: "end", behavior: "smooth" });
  }
  async function copyMessage(msg) {
    try {
      await navigator.clipboard.writeText(msg.text);
      copiedMsgId = msg.chatId;
      setTimeout(() => { if (copiedMsgId === msg.chatId) copiedMsgId = null; }, 1500);
    } catch (e) { flashError(e, "복사"); }
  }

  function flashError(rawOrMsg, contextHint) {
    const { kind, message } = toUserError(rawOrMsg, contextHint);
    if (kind === ErrorKind.CANCELLED) return;
    error = message;
    setTimeout(() => { if (error === message) error = ""; }, ERROR_CLEAR_MS);
    console.error(`[ChatView] ${kind}:`, rawOrMsg);
  }

  const IMG_EXTS = [".png", ".jpg", ".jpeg", ".gif", ".webp"];
  function avatarMimeType(name) {
    const ext = name.split(".").pop()?.toLowerCase();
    if (ext === "jpg" || ext === "jpeg") return "image/jpeg";
    if (ext === "gif") return "image/gif";
    if (ext === "webp") return "image/webp";
    return "image/png";
  }

  async function loadCharAvatar() {
    charAvatarUrl = "";
    charName = "";
    try {
      const char = await invoke("cmd_load_current_character");
      if (!char) return;
      const card = char.card;
      charName = card?.data?.name || char.characterName || "";
      const assetList = char.assetList || [];
      const imgAsset = assetList.find(a => IMG_EXTS.some(ext => a.name.toLowerCase().endsWith(ext)));
      if (imgAsset) {
        const b64 = await invoke("get_asset_data", { asset_name: imgAsset.name });
        if (b64) charAvatarUrl = `data:${avatarMimeType(imgAsset.name)};base64,${b64}`;
      }
    } catch (_) { /* no avatar available — fallback to initial */ }
  }

  let visibleMessages = $derived(getVisibleMessages(messages, branches, activeBranchId));
  function getState() { return { messages, activeToggles, branches, activeBranchId }; }

  const sessionAutosave = createAutosave(async () => {
    try { await saveSession(invoke, getState()); }
    catch (e) { console.error("Session save failed:", e); flashError(e, "세션 저장"); }
  }, { delayMs: 1000 });

  // ── Lifecycle ──────────────────────────────────────────────────────
  onMount(async () => {
    if (isTauri()) {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        unlisten = await listen("chat-chunk", (event) => { streamingText += event.payload; });
        unlistenAppFlush = await listen("app-flush", async () => {
          if (sessionLoaded) {
            try { await sessionAutosave.flush(); }
            catch (e) { console.error("ChatView app-flush save failed:", e); }
          }
        });
      } catch (e) { console.error("Event listen failed:", e); flashError(e, "이벤트 리스너 설정"); }
    }
    if (messages.length === 0) {
      const loaded = await loadSession(invoke, flashError);
      if (loaded) {
        if (loaded.messages) { messages = loaded.messages; branches = loaded.branches; activeBranchId = loaded.activeBranchId; }
        if (loaded.activeToggles) activeToggles = loaded.activeToggles;
      }
    }
    sessionLoaded = true;
    loadProviderLabel();
    loadCharAvatar();
    onReady?.({ sendChatMessage, getMessages: () => visibleMessages });
  });

  onDestroy(() => {
    if (unlisten) unlisten();
    if (unlistenAppFlush) unlistenAppFlush();
    if (sessionLoaded && messages.length > 0) {
      sessionAutosave.flush();
    } else {
      sessionAutosave.cancel();
    }
  });

  $effect(() => {
    messages; streamingText;
    if (chatBottom && isNearBottom) { requestAnimationFrame(() => { chatBottom.scrollIntoView({ block: "end", behavior: "smooth" }); }); }
  });

  $effect(() => {
    const _deps = [messages.length, activeToggles, branches, activeBranchId];
    if (sessionLoaded && messages.length > 0) {
      sessionAutosave.schedule();
    }
  });

  // ── Workspace switch: reload session when workspace changes ────────
  $effect(() => {
    const wsVersion = getWorkspaceVersion();
    // Skip the initial run (version 0 = no switch has happened yet)
    if (wsVersion === 0 || !sessionLoaded) return;
    // Re-load session from disk (switchWorkspace already wrote the new
    // workspace's session into the main session slot before bumping version).
    (async () => {
      sessionAutosave.cancel();
      const loaded = await loadSession(invoke, flashError);
      if (loaded) {
        if (loaded.messages) {
          messages = loaded.messages;
          branches = loaded.branches;
          activeBranchId = loaded.activeBranchId;
        } else {
          messages = [];
          branches = [{ id: MAIN_BRANCH_ID, name: "main", headId: null, forkPoint: null }];
          activeBranchId = MAIN_BRANCH_ID;
        }
        if (loaded.activeToggles) activeToggles = loaded.activeToggles;
        else activeToggles = {};
      } else {
        // No session found for this workspace — reset to empty
        messages = [];
        branches = [{ id: MAIN_BRANCH_ID, name: "main", headId: null, forkPoint: null }];
        activeBranchId = MAIN_BRANCH_ID;
        activeToggles = {};
      }
      greetings = [];
      greetingIndex = 0;
      forkDropdownId = null;
      editingMsgId = null;
      editingText = "";
      error = "";
      streamingText = "";
      loadProviderLabel();
      loadCharAvatar();
    })();
  });

  // ── Core chat logic ────────────────────────────────────────────────
  async function sendChatMessage(userMsg) {
    let loadedCharacter = null;
    try { loadedCharacter = await invoke("cmd_load_current_character"); }
    catch (e) { console.error("cmd_load_current_character failed:", e); }

    // Update avatar from loaded character if not yet loaded
    if (loadedCharacter && !charAvatarUrl) {
      charName = loadedCharacter.card?.data?.name || loadedCharacter.characterName || "";
      const assetList = loadedCharacter.assetList || [];
      const imgAsset = assetList.find(a => IMG_EXTS.some(ext => a.name.toLowerCase().endsWith(ext)));
      if (imgAsset) {
        invoke("get_asset_data", { asset_name: imgAsset.name }).then(b64 => {
          if (b64) charAvatarUrl = `data:${avatarMimeType(imgAsset.name)};base64,${b64}`;
        }).catch(() => {});
      }
    } else if (loadedCharacter) {
      charName = loadedCharacter.card?.data?.name || loadedCharacter.characterName || "";
    }

    if (messages.length === 0 && loadedCharacter) {
      const allGreetings = extractGreetings(loadedCharacter);
      if (allGreetings.length > 0) {
        greetings = allGreetings; greetingIndex = 0;
        const r = appendMessage(messages, branches, activeBranchId, "char", allGreetings[0], new Date().toLocaleTimeString());
        messages = r.messages; branches = r.branches;
      }
    }

    const userResult = appendMessage(messages, branches, activeBranchId, "user", userMsg, new Date().toLocaleTimeString());
    messages = userResult.messages; branches = userResult.branches;
    streaming = true; streamingText = "";
    const pollInterval = startPolling(invoke, (chunk) => { streamingText += chunk; });

    try {
      await invoke("start_streaming", { text: "AI 응답 수신 중..." }).catch(e => console.error("start_streaming failed:", e));
      const settings = await invoke("cmd_load_settings") || {};
      const provider = settings.chatProvider || "vertex";

      const pr = await assemblePrompt(invoke, flashError, { loadedCharacter, settings, activeToggles, hypaApi });
      let systemPrompt = pr.systemPrompt, postChatText = pr.postChatText;
      const loadedPreset = pr.loadedPreset;
      if (pr.toggleDefs.length > 0) { toggleDefs = pr.toggleDefs; activeToggles = pr.activeToggles; }
      else { toggleDefs = []; }

      let injectedUserMsg = userMsg;
      if (hypaApi?.getRagContext) {
        try {
          const qe = await embedQueryForRag(invoke, flashError, settings, userMsg);
          if (qe) {
            const hits = await hypaApi.getRagContext(qe, 5);
            if (Array.isArray(hits) && hits.length > 0) {
              const mem = hits.map(h => h?.summary?.text).filter(Boolean).join("\n\n");
              if (mem) injectedUserMsg = `[Memory]\n${mem}\n\n[User]\n${userMsg}`;
            }
          }
        } catch (e) { console.error("RAG injection failed:", e); flashError(e, "RAG 컨텍스트 조회"); }
      }

      let msgs = visibleMessages.filter(m => m.role !== "error").map((m, idx, arr) => ({
        role: m.role === "char" ? "model" : m.role,
        text: idx === arr.length - 1 && m.role === "user" ? injectedUserMsg : m.text,
      }));
      msgs = trimToContextWindow(msgs, loadedPreset?.maxContext, systemPrompt?.length || 0, postChatText?.length || 0);
      if (postChatText) msgs = [...msgs, { role: "user", text: postChatText }];

      const result = await dispatchChat(invoke, provider, settings, msgs, systemPrompt);
      const responseText = streamingText || result || "";
      if (responseText) {
        const cr = appendMessage(messages, branches, activeBranchId, "char", responseText, new Date().toLocaleTimeString());
        messages = cr.messages; branches = cr.branches;
        if (hypaApi?.triggerSummarizationIfNeeded) {
          const h = settings.hypa || {};
          hypaApi.triggerSummarizationIfNeeded(visibleMessages, h.summaryUnit).catch(e => { console.error("auto-summarize failed:", e); flashError(e, "자동 요약"); });
        }
      }
      return responseText;
    } catch (e) {
      const er = appendMessage(messages, branches, activeBranchId, "error", `Error: ${e}`, new Date().toLocaleTimeString());
      messages = er.messages; branches = er.branches;
      throw e;
    } finally { await stopPolling(invoke, pollInterval); streaming = false; streamingText = ""; }
  }

  // ── UI event handlers ──────────────────────────────────────────────
  async function sendMessage() {
    if (!chatInput.trim() || streaming) return;
    const userMsg = chatInput.trim(); chatInput = "";
    const ta = document.querySelector('.chat-input'); if (ta) ta.style.height = 'auto';
    loadProviderLabel();
    try { await sendChatMessage(userMsg); } catch (e) { console.error("sendMessage:", e); }
    requestAnimationFrame(() => { chatInputEl?.focus(); });
  }

  function autoResize(e) { const el = e.target; el.style.height = 'auto'; el.style.height = Math.min(el.scrollHeight, 120) + 'px'; }
  function handleChatKeydown(e) { if (e.key === "Enter" && !e.shiftKey && !/Android|iPhone|iPad/i.test(navigator.userAgent)) { e.preventDefault(); sendMessage(); } }

  async function clearChat() {
    const s = await clearSession(invoke, flashError, toggleDefs, activeToggles);
    messages = s.messages; branches = s.branches; activeBranchId = s.activeBranchId; activeToggles = s.activeToggles; forkDropdownId = null;
  }

  function deleteMessage(chatId) {
    const msg = messages.find(m => m.chatId === chatId);
    if (!msg) return;
    messages = messages.filter(m => m.chatId !== chatId).map(m => m.parentId === chatId ? { ...m, parentId: msg.parentId } : m);
    branches = branches.map(b => b.headId === chatId ? { ...b, headId: msg.parentId } : b);
  }

  async function regenerateResponse() {
    if (streaming || visibleMessages.length === 0) return;
    const prep = prepareRegeneration(visibleMessages, messages, branches, activeBranchId, visibleMessages.length - 1);
    if (!prep) return;
    messages = prep.messages; branches = prep.branches;
    try { await sendChatMessage(prep.savedUserText); } catch (e) { console.error("regenerateResponse:", e); }
    messages = reattachAlternatives(messages, getVisibleMessages(messages, branches, activeBranchId), prep.savedAlts);
  }

  async function regenerateFrom(targetMsg) {
    if (streaming) return;
    const visIdx = visibleMessages.findIndex(m => m.chatId === targetMsg.chatId);
    if (visIdx < 0) return;
    const prep = prepareRegeneration(visibleMessages, messages, branches, activeBranchId, visIdx);
    if (!prep) return;
    messages = prep.messages; branches = prep.branches;
    try { await sendChatMessage(prep.savedUserText); } catch (e) { console.error("regenerateFromMsg:", e); }
    messages = reattachAlternatives(messages, getVisibleMessages(messages, branches, activeBranchId), prep.savedAlts);
  }

  function swipeResponse(msg, direction) {
    if (!msg.alternatives?.length) return;
    if (!Array.isArray(msg._allResponses)) { msg._allResponses = [...msg.alternatives, msg.text]; msg._responseIdx = msg._allResponses.length - 1; }
    const total = msg._allResponses.length;
    msg._responseIdx = (msg._responseIdx + direction + total) % total;
    const newText = msg._allResponses[msg._responseIdx];
    const newAlts = msg._allResponses.filter((_, i) => i !== msg._responseIdx);
    messages = messages.map(m => m.chatId === msg.chatId ? { ...m, text: newText, alternatives: newAlts, _allResponses: msg._allResponses, _responseIdx: msg._responseIdx } : m);
  }

  function swipeGreeting(direction) {
    if (greetings.length < 2) return;
    greetingIndex = (greetingIndex + direction + greetings.length) % greetings.length;
    const first = visibleMessages.length > 0 ? visibleMessages[0] : null;
    if (first?.role === "char") messages = messages.map(m => m.chatId === first.chatId ? { ...m, text: greetings[greetingIndex] } : m);
  }

  async function cancelChat() { try { await invoke("cancel_chat"); } catch (e) { console.warn("cancel_chat:", e); } }
  function startEdit(msg) { editingMsgId = msg.chatId; editingText = msg.text; }
  function saveEdit(msg) { if (!editingText.trim()) return; messages = messages.map(m => m.chatId === msg.chatId ? { ...m, text: editingText } : m); editingMsgId = null; editingText = ""; }
  function cancelEdit() { editingMsgId = null; editingText = ""; }

  async function loadProviderLabel() {
    try { const s = await invoke("cmd_load_settings") || {}; const p = s.chatProvider || "vertex"; chatProvider = { vertex: "Vertex AI", gca: "GCA", mistral: "Mistral" }[p] || p; }
    catch (e) { console.warn("loadProviderLabel:", e); chatProvider = ""; }
  }

  async function pinMessage(msg) {
    const wasPinned = !!msg.pinned;
    // Optimistic UI toggle
    messages = messages.map(m => m.chatId === msg.chatId ? { ...m, pinned: !wasPinned } : m);
    try {
      await invoke("hypa_pin_message", { chat_id: msg.chatId, is_pinned: !wasPinned });
    } catch (e) {
      // Revert on failure
      messages = messages.map(m => m.chatId === msg.chatId ? { ...m, pinned: wasPinned } : m);
      console.error("hypa_pin_message failed:", e);
      flashError(e, "핀 설정");
    }
  }

  function altCount(msg) { return msg.alternatives?.length ? (msg._allResponses?.length ?? (msg.alternatives.length + 1)) : 0; }
  function forkFromMessage(chatId) { const r = createForkBranch(branches, chatId, `분기 ${branches.length}`); branches = r.branches; activeBranchId = r.newBranch.id; forkDropdownId = null; }
  function switchBranch(branchId) { activeBranchId = branchId; forkDropdownId = null; }
  function toggleForkDropdown(chatId) { forkDropdownId = forkDropdownId === chatId ? null : chatId; }
  function closeForkDropdown() { forkDropdownId = null; }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="chat-view" onclick={closeForkDropdown}>
  {#if error}
    <div class="chat-error-toast">{error}</div>
  {/if}

  {#if branches.length > 1}
    <div class="branch-bar">
      <svg class="branch-bar-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M6 3v12M18 9a3 3 0 100-6 3 3 0 000 6zM6 21a3 3 0 100-6 3 3 0 000 6zM18 9a9 9 0 01-9 9" />
      </svg>
      <select class="branch-select" value={activeBranchId} onchange={(e) => switchBranch(e.target.value)}>
        {#each branches as branch}
          <option value={branch.id}>{branch.name}{branch.id === activeBranchId ? " (현재)" : ""}</option>
        {/each}
      </select>
      <span class="branch-count">{branches.length}개 브랜치</span>
    </div>
  {/if}

  <div class="chat-messages" bind:this={chatMessagesEl} onscroll={handleMessagesScroll}>
    {#if visibleMessages.length === 0 && !streaming}
      <div class="empty-state">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
        </svg>
        <p>프롬프트를 테스트할 대화를 시작하세요</p>
        <ol class="empty-steps">
          <li>Settings에서 API 프로바이더 설정</li>
          <li>Character 탭에서 캐릭터 로드</li>
          <li>Preset 탭에서 프리셋 로드</li>
          <li>아래 입력창에 메시지 전송</li>
        </ol>
      </div>
    {/if}

    {#each visibleMessages as msg, i}
      {@const forkCount = countForks(branches, msg.chatId)}
      {@const branchesHere = getBranchesAtForkPoint(branches, msg.chatId)}
      <div class="message {msg.role}" class:pinned={msg.pinned}>
        {#if msg.role !== "error"}
          <div class="msg-actions">
            <button class="msg-action-btn msg-fork-btn" onclick={(e) => { e.stopPropagation(); forkFromMessage(msg.chatId); }} disabled={streaming} title="포크 (분기 생성)" aria-label="이 메시지에서 분기">
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                <path d="M6 3v12M18 9a3 3 0 100-6 3 3 0 000 6zM6 21a3 3 0 100-6 3 3 0 000 6zM18 9a9 9 0 01-9 9" />
              </svg>
            </button>
            {#if msg.role === "char"}
              <button class="msg-action-btn msg-pin-btn" class:pinned={msg.pinned} onclick={() => pinMessage(msg)} disabled={streaming} title={msg.pinned ? "핀 해제" : "핀"} aria-label={msg.pinned ? "핀 해제" : "핀 고정"}>
                <svg width="12" height="12" viewBox="0 0 24 24" fill={msg.pinned ? "currentColor" : "none"} stroke="currentColor" stroke-width="2">
                  <path d="M12 2L8.5 8.5 2 9.5l4.5 5L5.5 22 12 18.5 18.5 22l-1-7.5 4.5-5-6.5-1z" />
                </svg>
              </button>
              <button class="msg-action-btn msg-regen-btn" onclick={() => regenerateFrom(msg)} disabled={streaming} title="여기서 재생성" aria-label="이 응답부터 재생성">↻</button>
            {/if}
            <button class="msg-action-btn" onclick={() => copyMessage(msg)} title={copiedMsgId === msg.chatId ? "복사됨" : "복사"}>{copiedMsgId === msg.chatId ? "✓" : "📋"}</button>
            <button class="msg-action-btn" onclick={() => startEdit(msg)} disabled={streaming || editingMsgId !== null} title="편집" aria-label="메시지 편집">✏</button>
            <button class="msg-action-btn msg-delete-btn" onclick={() => deleteMessage(msg.chatId)} disabled={streaming} title="삭제" aria-label="메시지 삭제">×</button>
          </div>
        {/if}
        {#if msg.chatId === editingMsgId}
          <div class="edit-area">
            <ResizableTextarea className="edit-textarea" bind:value={editingText} minHeight={120} />
            <div class="edit-buttons">
              <button class="btn btn-sm btn-primary edit-save-btn" onclick={() => saveEdit(msg)}>저장</button>
              <button class="btn btn-sm btn-secondary edit-cancel-btn" onclick={cancelEdit}>취소</button>
            </div>
          </div>
        {:else if msg.role === "error"}
          <div class="message-bubble error-bubble">
            <svg class="error-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="12" cy="12" r="10" /><path d="M12 8v4M12 16h.01" />
            </svg>
            <span>{msg.text}</span>
          </div>
          <div class="error-actions">
            <button class="btn btn-sm btn-danger error-retry-btn" onclick={regenerateResponse} disabled={streaming}>다시 시도</button>
          </div>
        {:else if msg.role === "char"}
          <div class="msg-content-row">
            <div class="msg-avatar char-avatar" aria-hidden="true">
              {#if charAvatarUrl}
                <img src={charAvatarUrl} alt="" class="avatar-img" />
              {:else}
                <span class="avatar-initial">{charName ? charName[0].toUpperCase() : "C"}</span>
              {/if}
            </div>
            <div class="message-bubble">{@html renderMarkdown(msg.text)}</div>
          </div>
        {:else}
          <div class="msg-content-row user-row">
            <div class="message-bubble">{msg.text}</div>
            <div class="msg-avatar user-avatar" aria-hidden="true">
              <span class="avatar-initial">U</span>
            </div>
          </div>
        {/if}

        {#if msg.role !== "error"}
          <span class="message-time">
            {msg.time}
            {#if msg.pinned}
              <span class="pin-badge">
                <svg width="10" height="10" viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="1.5">
                  <path d="M12 2L8.5 8.5 2 9.5l4.5 5L5.5 22 12 18.5 18.5 22l-1-7.5 4.5-5-6.5-1z" />
                </svg>
                핀
              </span>
            {/if}
            {#if msg.role === "char" && altCount(msg) > 0}
              <span class="alt-badge">{altCount(msg)}</span>
            {/if}
          </span>
        {:else}
          <span class="message-time">{msg.time}</span>
        {/if}

        {#if forkCount > 0}
          <div class="fork-indicator" style="position: relative;">
            <button class="fork-badge" onclick={(e) => { e.stopPropagation(); toggleForkDropdown(msg.chatId); }} aria-label="브랜치 목록 보기">
              <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                <path d="M6 3v12M18 9a3 3 0 100-6 3 3 0 000 6zM6 21a3 3 0 100-6 3 3 0 000 6zM18 9a9 9 0 01-9 9" />
              </svg>
              {forkCount + 1}개 분기
            </button>
            {#if forkDropdownId === msg.chatId}
              <div class="fork-dropdown" onclick={(e) => e.stopPropagation()}>
                {#each branchesHere as branch}
                  <button class="fork-dropdown-item" class:active={branch.id === activeBranchId} onclick={() => switchBranch(branch.id)}>
                    <span class="fork-dropdown-name">{branch.name}</span>
                    {#if branch.id === activeBranchId}<span class="fork-dropdown-current">현재</span>{/if}
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        {/if}

        {#if i === 0 && msg.role === "char" && greetings.length >= 2}
          <div class="swipe-nav">
            <button onclick={() => swipeGreeting(-1)} aria-label="이전 인사말">◀</button>
            <span>{greetingIndex + 1}/{greetings.length}</span>
            <button onclick={() => swipeGreeting(1)} aria-label="다음 인사말">▶</button>
          </div>
        {:else if msg.role === "char" && msg.alternatives?.length}
          {@const total = msg._allResponses?.length ?? ((msg.alternatives?.length || 0) + 1)}
          {@const current = (msg._responseIdx ?? (total - 1)) + 1}
          <div class="swipe-nav">
            <button onclick={() => swipeResponse(msg, -1)} disabled={streaming} aria-label="이전 응답">◀</button>
            <span>{current}/{total}</span>
            <button onclick={() => swipeResponse(msg, 1)} disabled={streaming} aria-label="다음 응답">▶</button>
          </div>
        {/if}
      </div>
    {/each}

    {#if streaming}
      <div class="message char">
        <div class="msg-content-row">
          <div class="msg-avatar char-avatar" aria-hidden="true">
            {#if charAvatarUrl}
              <img src={charAvatarUrl} alt="" class="avatar-img" />
            {:else}
              <span class="avatar-initial">{charName ? charName[0].toUpperCase() : "C"}</span>
            {/if}
          </div>
          <div class="message-bubble">
            {#if streamingText}{@html renderMarkdown(streamingText)}{:else}<div class="spinner" style="margin: 4px auto;"></div>{/if}
          </div>
        </div>
      </div>
    {/if}

    {#if !isNearBottom && visibleMessages.length > 0}
      <button class="scroll-to-bottom" onclick={scrollToBottom} title="최신 메시지로 이동" aria-label="최신 메시지로 스크롤">
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><path d="M6 9l6 6 6-6" /></svg>
      </button>
    {/if}

    <div bind:this={chatBottom} aria-hidden="true"></div>
  </div>

  <div class="chat-input-bar">
    {#if toggleDefs.length > 0}
      <div class="toggle-panel-wrapper">
        <button class="toggle-panel-trigger" onclick={() => { togglePanelOpen = !togglePanelOpen; }} aria-expanded={togglePanelOpen} aria-label="프롬프트 토글 패널 열기/닫기">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 5v14M5 12h14" /></svg>
          토글 ({Object.values(activeToggles).filter(Boolean).length}/{toggleDefs.length})
          <svg class="toggle-panel-chevron" class:open={togglePanelOpen} width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M6 9l6 6 6-6" /></svg>
        </button>
        {#if togglePanelOpen}
          <div class="toggle-panel">
            {#each toggleDefs as def (def.key)}
              <label class="toggle-panel-item">
                <span class="toggle-panel-label">{def.label}</span>
                <span class="toggle">
                  <input type="checkbox" checked={activeToggles[def.key] !== false} onchange={(e) => { activeToggles[def.key] = e.target.checked; activeToggles = {...activeToggles}; }} />
                  <span class="toggle-track"></span>
                </span>
              </label>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
    <div class="input-meta-row">
      {#if chatProvider}<span class="provider-badge">{chatProvider}</span>{/if}
      {#if branches.length > 1}<span class="branch-badge">{branches.find(b => b.id === activeBranchId)?.name || activeBranchId}</span>{/if}
      {#if chatInput.length > 0}<span class="char-count">{chatInput.length}자</span>{/if}
    </div>
    <div class="input-row">
      {#if visibleMessages.length > 0}
        <button class="btn-icon" onclick={clearChat} disabled={streaming} title="대화 비우기" aria-label="대화 비우기" style="flex-shrink: 0; align-self: center;"><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18M8 6V4a2 2 0 012-2h4a2 2 0 012 2v2m3 0v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6h14z" /></svg></button>
      {/if}
      {#if visibleMessages.length > 0 && visibleMessages[visibleMessages.length - 1].role === "char"}
        <button class="btn-icon" onclick={regenerateResponse} disabled={streaming} title="응답 재생성" aria-label="응답 재생성" style="flex-shrink: 0; align-self: center;"><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M23 4v6h-6M1 20v-6h6" /><path d="M3.51 9a9 9 0 0114.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0020.49 15" /></svg></button>
      {/if}
      <textarea class="chat-input" rows="1" placeholder="메시지를 입력하세요..." bind:value={chatInput} bind:this={chatInputEl} onkeydown={handleChatKeydown} oninput={autoResize} style="height: auto; min-height: 36px;"></textarea>
      {#if streaming}
        <button class="send-btn cancel" onclick={cancelChat} title="취소">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="6" width="12" height="12" rx="2" /></svg>
        </button>
      {:else}
        <button class="send-btn" onclick={sendMessage} disabled={!chatInput.trim()} title="전송" aria-label="메시지 전송">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor"><path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z" /></svg>
        </button>
      {/if}
    </div>
    <div style="position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0,0,0,0)" role="status" aria-live="polite">{streaming ? "응답 생성 중..." : ""}</div>
  </div>
</div>

<style>
  @import './ChatView.css';
</style>
