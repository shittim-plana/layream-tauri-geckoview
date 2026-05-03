<script>
  import { invoke } from "../lib/tauri.js";

  // chatApi: { sendChatMessage, getMessages } registered by parent from ChatView.
  let { chatApi } = $props();

  let autopilotRunning = $state(false);
  let autopilotTurns = $state(5);
  let autopilotStrategy = $state("continue");
  let autopilotMessages = $state("");
  let autopilotLog = $state([]);
  let autopilotProvider = $state("same");
  let autopilotModel = $state("");

  async function toggleAutopilot() {
    if (autopilotRunning) {
      autopilotRunning = false;
      autopilotLog = [...autopilotLog, { turn: 0, status: "Stopped by user", time: new Date().toLocaleTimeString() }];
      return;
    }

    if (!chatApi?.sendChatMessage) {
      autopilotLog = [{ turn: 0, status: "Chat not ready", time: new Date().toLocaleTimeString() }];
      return;
    }

    autopilotRunning = true;
    autopilotLog = [{ turn: 0, status: "Started", time: new Date().toLocaleTimeString() }];

    for (let turn = 1; turn <= autopilotTurns && autopilotRunning; turn++) {
      let userMsg = "(계속)";

      if (autopilotStrategy === "predefined") {
        const lines = autopilotMessages.split("\n").filter(l => l.trim());
        userMsg = lines[(turn - 1) % lines.length] || "(계속)";
      } else if (autopilotStrategy === "ai") {
        try {
          const lastMsgs = (chatApi.getMessages?.() || []).slice(-6);
          const settings = await invoke("cmd_load_settings") || {};
          const provider = autopilotProvider === "same" ? (settings.chatProvider || "vertex") : autopilotProvider;
          const model = autopilotModel || (provider === "vertex" ? (settings.vertexModel || "gemini-2.5-flash") :
                        provider === "gca" ? (settings.gcaModel || "gemini-2.5-flash") :
                        (settings.mistralModel || "mistral-small-2603"));

          userMsg = await invoke("generate_user_message", {
            context: lastMsgs,
            provider,
            model,
            region: settings.vertexRegion || "us-central1",
            project_id: settings.vertexProjectId || "",
            api_key: settings.mistralKey || "",
          });
        } catch (e) {
          userMsg = "(AI 생성 실패, 계속)";
          autopilotLog = [...autopilotLog, { turn, status: `AI gen failed: ${e}`, time: new Date().toLocaleTimeString() }];
        }
      }

      try {
        autopilotLog = [...autopilotLog, { turn, status: `Sending: ${userMsg.slice(0, 50)}...`, time: new Date().toLocaleTimeString() }];
        const response = await chatApi.sendChatMessage(userMsg);
        autopilotLog = [...autopilotLog, { turn, status: `Response: ${(response || "").length} chars`, time: new Date().toLocaleTimeString() }];
      } catch (e) {
        autopilotLog = [...autopilotLog, { turn, status: `Error: ${e}`, time: new Date().toLocaleTimeString() }];
        break;
      }
    }

    if (autopilotRunning) {
      autopilotLog = [...autopilotLog, { turn: 0, status: "Completed", time: new Date().toLocaleTimeString() }];
    }
    autopilotRunning = false;
  }
</script>

<div class="card">
  <div class="card-header">
    <span class="card-title">Autopilot Settings</span>
    <button class="btn btn-sm {autopilotRunning ? 'btn-danger' : 'btn-primary'}" onclick={toggleAutopilot}>
      {autopilotRunning ? "Stop" : "Start"}
    </button>
  </div>
  <div class="card-body">
    <div class="field">
      <label class="label">Turns (1-50)</label>
      <input class="input" type="number" min="1" max="50" bind:value={autopilotTurns} disabled={autopilotRunning} />
    </div>
    <div class="field">
      <label class="label">User Message Strategy</label>
      <select class="select" bind:value={autopilotStrategy} disabled={autopilotRunning}>
        <option value="continue">Continue (empty message)</option>
        <option value="predefined">Predefined messages</option>
        <option value="ai">AI-generated</option>
      </select>
    </div>
    {#if autopilotStrategy === "predefined"}
      <div class="field">
        <label class="label">Messages (one per line)</label>
        <textarea class="textarea" rows="4" bind:value={autopilotMessages} disabled={autopilotRunning} placeholder="Hello&#10;How are you?&#10;Tell me more"></textarea>
      </div>
    {/if}
    {#if autopilotStrategy === "ai"}
      <div class="field">
        <label class="label">AI Generation Provider</label>
        <select class="select" bind:value={autopilotProvider} disabled={autopilotRunning}>
          <option value="same">Same as Chat</option>
          <option value="vertex">Vertex AI</option>
          <option value="gca">GCA</option>
          <option value="mistral">Mistral</option>
        </select>
      </div>
      {#if autopilotProvider !== "same"}
        <div class="field">
          <label class="label">Model (optional)</label>
          <input class="input" type="text" bind:value={autopilotModel} disabled={autopilotRunning} placeholder="Uses default model" />
        </div>
      {/if}
    {/if}
  </div>
</div>

{#if autopilotLog.length > 0}
  <div class="card">
    <div class="card-header"><span class="card-title">Execution Log</span></div>
    <div class="card-body" style="max-height: 300px; overflow-y: auto;">
      {#each autopilotLog as entry}
        <div style="font-size: 12px; padding: 4px 0; border-bottom: 1px solid var(--bg4); color: var(--fg2);">
          <span style="color: var(--fg3);">{entry.time}</span> — {entry.status}
        </div>
      {/each}
    </div>
  </div>
{/if}
