<script>
  import { invoke } from "../lib/tauri.js";

  let messages = $state([]);
  let input = $state("");
  let streaming = $state(false);
  let streamText = $state("");

  async function sendMessage() {
    if (!input.trim() || streaming) return;
    const userMsg = input.trim();
    input = "";
    messages = [...messages, { role: "user", text: userMsg }];
    streaming = true;
    streamText = "";

    try {
      const result = await invoke("chat_send", { message: userMsg });
      if (result) {
        messages = [...messages, { role: "model", text: result }];
      }
    } catch (e) {
      messages = [...messages, { role: "error", text: String(e) }];
    }
    streaming = false;
  }

  function handleKeydown(e) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  }

  function clearChat() {
    messages = [];
  }
</script>

<div style="display: flex; flex-direction: column; height: calc(100vh - 120px);">
  <div style="flex: 1; overflow-y: auto; padding-bottom: 12px;">
    {#if messages.length === 0}
      <div style="text-align: center; padding: 48px; color: var(--text-dim);">
        <p>Start a conversation to test your prompts</p>
        <p style="font-size: 12px; margin-top: 8px;">
          Configure Vertex AI in Settings first
        </p>
      </div>
    {/if}

    {#each messages as msg}
      <div
        class="card"
        style="margin: 4px 0; border-left: 3px solid {msg.role === 'user'
          ? 'var(--primary)'
          : msg.role === 'error'
            ? 'var(--accent)'
            : '#50fa7b'};"
      >
        <div style="font-size: 11px; color: var(--text-dim); margin-bottom: 4px; text-transform: uppercase;">
          {msg.role}
        </div>
        <div style="white-space: pre-wrap; font-size: 14px;">{msg.text}</div>
      </div>
    {/each}

    {#if streaming}
      <div class="card" style="border-left: 3px solid #ffb86c;">
        <div style="font-size: 11px; color: var(--text-dim);">GENERATING...</div>
        <div style="white-space: pre-wrap;">{streamText || "..."}</div>
      </div>
    {/if}
  </div>

  <div style="display: flex; gap: 8px; padding: 8px 0;">
    <textarea
      rows="2"
      style="flex: 1; min-height: 40px;"
      placeholder="Type a message..."
      bind:value={input}
      onkeydown={handleKeydown}
    ></textarea>
    <div style="display: flex; flex-direction: column; gap: 4px;">
      <button onclick={sendMessage} disabled={streaming || !input.trim()}>Send</button>
      <button onclick={clearChat} style="font-size: 12px;">Clear</button>
    </div>
  </div>
</div>
