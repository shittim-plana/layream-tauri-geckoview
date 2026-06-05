<script>
  import { invoke } from "../lib/tauri.js";

  // chatApi: { sendChatMessage, getMessages } registered by parent from ChatView.
  let { chatApi } = $props();

  // --- Constants ---
  const TURN_MIN = 1;
  const TURN_MAX = 50;
  const PAUSE_POLL_MS = 500; // how often we re-check FSM state while paused
  const AI_CONTEXT_WINDOW = 6; // last N messages passed to user-message generator

  // --- v1 state ---
  let autopilotTurns = $state(5);
  let autopilotStrategy = $state("continue");
  let autopilotMessages = $state("");
  let autopilotLog = $state([]);
  let autopilotProvider = $state("same");
  let autopilotModel = $state("");

  // --- v2 state ---
  // FSM: replaces autopilotRunning:boolean. Transitions:
  //   stopped --Start--> running
  //   running --Pause--> paused
  //   running --Stop--> stopped
  //   paused  --Resume-> running
  //   paused  --Stop--> stopped
  //   running --(turns drained | error)--> stopped
  let autopilotState = $state("stopped"); // "stopped" | "running" | "paused"

  // A-1: user persona prepended to USER_MSG_SYSTEM_PROMPT in ai strategy.
  let autopilotPersona = $state("");

  // A-2: char↔char mode. SEMANTICS — this is "personas alternate, both AI",
  // NOT a true AI↔AI dialogue. Every generated message is still routed through
  // chatApi.sendChatMessage(...), which appends the input as role:"user" and
  // then triggers a char response from the chat provider. So a "char-to-char"
  // turn looks like: (a) generate_user_message runs with the *char* persona,
  // producing text from the char's POV; (b) that text is sent as a user-role
  // message; (c) the chat provider replies in-character. This is a deliberate
  // pragmatic choice: ChatView owns message-array state and exposes only
  // sendChatMessage as the conduit (no char-side append), so true alternation
  // would require modifying ChatView. Until that API exists, "char-to-char"
  // means "alternate which persona drives generate_user_message each turn"
  // — both halves of the dialogue still pass through the user-input pipeline.
  let autopilotMode = $state("user-driven"); // "user-driven" | "char-to-char"
  let autopilotCharPersona = $state("");

  // A-4: structured output. JSON schema string; parsed lazily on use.
  let autopilotResponseSchema = $state("");
  let autopilotStructuredEnabled = $state(false);
  let autopilotSchemaError = $state("");

  // --- Derived ---
  // Pure derivations (§5-D): button state from FSM. No stored duplicates.
  let isStopped = $derived(autopilotState === "stopped");
  let isRunning = $derived(autopilotState === "running");
  let isPaused = $derived(autopilotState === "paused");
  // Inputs are locked except in "stopped" so that mid-run param mutation
  // doesn't desync the loop's reading of state.
  let inputsLocked = $derived(!isStopped);
  let statusMessage = $derived(isRunning ? "실행 중" : isPaused ? "일시정지" : "대기 중");

  // --- Schema validation ---
  // Returns { schema, error }. error="" on success or empty input.
  function parseSchema(text) {
    const trimmed = (text || "").trim();
    if (!trimmed) return { schema: null, error: "" };
    try {
      const parsed = JSON.parse(trimmed);
      // JSON Schema must be a plain object — reject null, arrays, primitives.
      // Array is a typeof "object" in JS so it needs its own check (§4-C:
      // accepting an array would fabricate an object-shaped contract).
      if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
        return { schema: null, error: "Schema must be a JSON object" };
      }
      return { schema: parsed, error: "" };
    } catch (e) {
      return { schema: null, error: `Invalid JSON: ${e.message || e}` };
    }
  }

  $effect(() => {
    // Re-validate whenever the schema text changes; user sees error inline.
    const { error } = parseSchema(autopilotResponseSchema);
    autopilotSchemaError = error;
  });

  // --- Logging helper ---
  // `turn` is a 1-based loop index for per-turn events, or `null` for FSM-level
  // events (Started, Paused, Resumed, Stopped, Completed, Halted) that are not
  // associated with any specific turn.
  function log(turn, status) {
    autopilotLog = [...autopilotLog, { turn, status, time: new Date().toLocaleTimeString() }];
  }

  // --- FSM actions ---
  function startAutopilot() {
    if (!isStopped) return;
    if (!chatApi?.sendChatMessage) {
      autopilotLog = [{ turn: null, status: "Chat not ready", time: new Date().toLocaleTimeString() }];
      return;
    }
    if (autopilotStructuredEnabled && autopilotSchemaError) {
      log(null, `Cannot start: ${autopilotSchemaError}`);
      return;
    }
    autopilotLog = [{ turn: null, status: "Started", time: new Date().toLocaleTimeString() }];
    autopilotState = "running";
    // fire-and-forget; loop ends when state becomes "stopped".
    // .catch guards against synchronous throws surfacing as unhandled rejections.
    runAutopilotLoop().catch((e) => log(null, "Loop crash: " + e));
  }

  function pauseAutopilot() {
    if (!isRunning) return;
    autopilotState = "paused";
    log(null, "Paused");
  }

  function resumeAutopilot() {
    if (!isPaused) return;
    autopilotState = "running";
    log(null, "Resumed");
  }

  function stopAutopilot() {
    if (isStopped) return;
    autopilotState = "stopped";
    log(null, "Stopped by user");
  }

  // --- Generation arg builder ---
  // Encapsulates the per-turn invoke args. `persona` is the active persona for
  // this turn's generator (user or char depending on mode). Extra fields are
  // ignored by the current backend (serde default; §1-C: forward-compatible).
  async function buildGenArgs(turnPersona) {
    const lastMsgs = (chatApi.getMessages?.() || []).slice(-AI_CONTEXT_WINDOW);
    const settings = (await invoke("cmd_load_settings")) || {};
    const provider = autopilotProvider === "same"
      ? (settings.chatProvider || "vertex")
      : autopilotProvider;
    const model = autopilotModel || (
      provider === "vertex" ? (settings.vertexModel || "gemini-2.5-flash") :
      provider === "gca"    ? (settings.gcaModel    || "gemini-2.5-flash") :
                              (settings.mistralModel || "mistral-small-2603")
    );
    const args = {
      context: lastMsgs,
      provider,
      model,
      region: settings.vertexRegion || "us-central1",
      project_id: settings.vertexProjectId || "",
      api_key: settings.mistralKey || "",
    };
    if (turnPersona && turnPersona.trim()) {
      args.persona = turnPersona;
    }
    if (autopilotStructuredEnabled) {
      const { schema } = parseSchema(autopilotResponseSchema);
      if (schema) args.response_schema = schema;
    }
    return args;
  }

  // Pick the persona for the current turn. In char-to-char mode the persona
  // alternates: turn 1 = user, turn 2 = char, turn 3 = user, ... so that each
  // side's generator sees the other side as the conversational partner.
  function personaForTurn(turn) {
    if (autopilotMode !== "char-to-char") return autopilotPersona;
    return turn % 2 === 1 ? autopilotPersona : autopilotCharPersona;
  }

  // --- Pause-aware wait ---
  // Yields control while paused; returns true if loop should continue, false
  // if user requested stop. Caller uses the return value to break the loop.
  async function waitWhilePaused() {
    while (autopilotState === "paused") {
      await new Promise((r) => setTimeout(r, PAUSE_POLL_MS));
    }
    return autopilotState === "running";
  }

  // --- Main loop ---
  async function runAutopilotLoop() {
    // Clamp turns to [TURN_MIN, TURN_MAX] integer range at loop start (§9-A:
    // make the bound explicit; §1-E: NaN/negative/>MAX are not valid inputs).
    // `| 0` coerces NaN/floats to int; Math.max/min then clamps the range.
    const turns = Math.max(TURN_MIN, Math.min(TURN_MAX, autopilotTurns | 0));
    let errored = false;
    for (let turn = 1; turn <= turns; turn++) {
      // Check FSM at top of every turn (§2-A: pause boundary).
      if (!(await waitWhilePaused())) break;

      let userMsg = "(계속)";

      if (autopilotStrategy === "predefined") {
        const lines = autopilotMessages.split("\n").filter((l) => l.trim());
        userMsg = lines.length > 0 ? lines[(turn - 1) % lines.length] : "(계속)";
      } else if (autopilotStrategy === "ai") {
        try {
          const args = await buildGenArgs(personaForTurn(turn));
          userMsg = await invoke("generate_user_message", args);
        } catch (e) {
          userMsg = "(AI 생성 실패, 계속)";
          log(turn, `AI gen failed: ${e}`);
        }
      }
      // "continue" strategy: userMsg stays "(계속)".

      // Re-check after potentially long AI gen (user may have paused/stopped).
      if (!(await waitWhilePaused())) break;

      try {
        log(turn, `Sending: ${userMsg.slice(0, 50)}${userMsg.length > 50 ? "..." : ""}`);
        const response = await chatApi.sendChatMessage(userMsg);
        log(turn, `Response: ${(response || "").length} chars`);
      } catch (e) {
        log(turn, `Error: ${e}`);
        errored = true;
        break;
      }
    }

    // Natural completion or error break: drain to "stopped" unless already stopped.
    if (autopilotState !== "stopped") {
      log(null, errored ? "Halted on error" : "Completed");
      autopilotState = "stopped";
    }
  }
</script>

<div class="card">
  <div class="card-header">
    <span class="card-title">Autopilot Settings</span>
    <div style="display: flex; gap: 6px;">
      {#if isStopped}
        <button class="btn btn-sm btn-primary" onclick={startAutopilot}>Start</button>
      {:else if isRunning}
        <button class="btn btn-sm btn-secondary" onclick={pauseAutopilot}>Pause</button>
        <button class="btn btn-sm btn-danger" onclick={stopAutopilot}>Stop</button>
      {:else if isPaused}
        <button class="btn btn-sm btn-primary" onclick={resumeAutopilot}>Resume</button>
        <button class="btn btn-sm btn-danger" onclick={stopAutopilot}>Stop</button>
      {/if}
    </div>
  </div>
  <div class="card-body">
    <div class="status-row" role="status" aria-live="polite">
      <span style="font-size: 13px; color: var(--fg);">Autopilot 상태: <strong>{statusMessage}</strong></span>
    </div>
    <div class="field">
      <label class="label">Turns ({TURN_MIN}-{TURN_MAX})</label>
      <input class="input" type="number" min={TURN_MIN} max={TURN_MAX} bind:value={autopilotTurns} disabled={inputsLocked} />
    </div>

    <div class="field">
      <label class="label">Mode</label>
      <select class="select" bind:value={autopilotMode} disabled={inputsLocked}>
        <option value="user-driven">User-driven (user → char)</option>
        <option value="char-to-char">Character ↔ Character</option>
      </select>
    </div>

    <div class="field">
      <label class="label">User Message Strategy</label>
      <select class="select" bind:value={autopilotStrategy} disabled={inputsLocked}>
        <option value="continue">Continue (empty message)</option>
        <option value="predefined">Predefined messages</option>
        <option value="ai">AI-generated</option>
      </select>
    </div>

    {#if autopilotStrategy === "predefined"}
      <div class="field">
        <label class="label">Messages (one per line)</label>
        <textarea class="textarea" rows="4" bind:value={autopilotMessages} disabled={inputsLocked} placeholder="Hello&#10;How are you?&#10;Tell me more"></textarea>
      </div>
    {/if}

    {#if autopilotStrategy === "ai"}
      <div class="field">
        <label class="label">AI Generation Provider</label>
        <select class="select" bind:value={autopilotProvider} disabled={inputsLocked}>
          <option value="same">Same as Chat</option>
          <option value="vertex">Vertex AI</option>
          <option value="gca">GCA</option>
          <option value="mistral">Mistral</option>
        </select>
      </div>
      {#if autopilotProvider !== "same"}
        <div class="field">
          <label class="label">Model (optional)</label>
          <input class="input" type="text" bind:value={autopilotModel} disabled={inputsLocked} placeholder="Uses default model" />
        </div>
      {/if}

      <div class="field">
        <label class="label">User Persona</label>
        <textarea class="textarea" rows="3" bind:value={autopilotPersona} disabled={inputsLocked} placeholder="e.g. A curious traveler asking about local customs"></textarea>
      </div>

      {#if autopilotMode === "char-to-char"}
        <div class="field">
          <label class="label">Character Persona</label>
          <textarea class="textarea" rows="3" bind:value={autopilotCharPersona} disabled={inputsLocked} placeholder="e.g. A wise elder who answers in riddles"></textarea>
        </div>
      {/if}

      <div class="field">
        <label class="label" style="display: flex; align-items: center; gap: 8px;">
          <input type="checkbox" bind:checked={autopilotStructuredEnabled} disabled={inputsLocked} />
          Structured Output (advanced)
        </label>
        {#if autopilotStructuredEnabled}
          <textarea
            class="textarea"
            rows="5"
            bind:value={autopilotResponseSchema}
            disabled={inputsLocked}
            placeholder={'{\n  "type": "object",\n  "properties": { "message": { "type": "string" } }\n}'}
          ></textarea>
          {#if autopilotSchemaError}
            <p style="font-size: 11px; color: var(--red); margin-top: 4px;">{autopilotSchemaError}</p>
          {:else if autopilotResponseSchema.trim()}
            <p style="font-size: 11px; color: var(--green); margin-top: 4px;">Schema valid</p>
          {/if}
        {/if}
      </div>
    {/if}
  </div>
</div>

{#if autopilotLog.length > 0}
  <div class="card">
    <div class="card-header">
      <span class="card-title">Execution Log</span>
      <button class="btn btn-sm btn-secondary" onclick={() => (autopilotLog = [])} disabled={isRunning}>Clear</button>
    </div>
    <div class="card-body" style="max-height: 300px; overflow-y: auto;">
      {#each autopilotLog as entry}
        <div style="font-size: 12px; padding: 4px 0; border-bottom: 1px solid var(--bg4); color: var(--fg2);">
          <span style="color: var(--fg3);">{entry.time}</span> — {entry.status}
        </div>
      {/each}
    </div>
  </div>
{:else}
  <div class="card"><div class="card-body"><div class="empty-state" style="padding: 28px 20px;">
    <p>실행 로그가 없습니다.</p>
    <p class="section-note">Start 버튼으로 테스트 루프를 실행해보세요.</p>
  </div></div></div>
{/if}
