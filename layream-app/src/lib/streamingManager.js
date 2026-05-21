/** Start polling for stream chunks. Returns the interval id.
 *  @param {Function} invoke - Tauri invoke function
 *  @param {Function} onChunks - callback receiving new text chunks */
export function startPolling(invoke, onChunks) {
  return setInterval(async () => {
    try {
      const chunks = await invoke("poll_stream_chunks");
      if (chunks?.length) {
        for (const chunk of chunks) onChunks(chunk);
      }
    } catch (e) { console.warn("poll_stream_chunks:", e); }
  }, 100);
}

/** Stop polling and clean up the streaming session.
 *  @param {Function} invoke - Tauri invoke function
 *  @param {number|null} pollInterval - interval id to clear */
export async function stopPolling(invoke, pollInterval) {
  if (pollInterval != null) clearInterval(pollInterval);
  await invoke("stop_streaming").catch(e => console.error("stop_streaming failed:", e));
}

/** Dispatch a chat request to the configured provider.
 *  @param {Function} invoke - Tauri invoke function
 *  @param {string} provider - "vertex" | "gca" | "mistral"
 *  @param {object} settings - full app settings
 *  @param {object[]} msgs - message array for the API
 *  @param {string|null} systemPrompt
 *  @returns {Promise<string>} response text from the provider */
export async function dispatchChat(invoke, provider, settings, msgs, systemPrompt) {
  if (provider === "vertex") {
    const c = settings.vertexConfig || {};
    return invoke("chat_vertex", {
      messages: msgs,
      system_prompt: systemPrompt,
      model: settings.vertexModel || "gemini-2.5-flash",
      project_id: settings.vertexProjectId || "",
      region: settings.vertexRegion || "us-central1",
      temperature: c.temperature ?? 0.9,
      max_tokens: c.max_tokens ?? 8192,
      top_p: c.top_p ?? null,
      top_k: c.top_k ?? null,
      frequency_penalty: c.frequency_penalty ?? null,
      presence_penalty: c.presence_penalty ?? null,
      thinking_budget: c.thinking_budget ?? null,
      tools_google_search: c.tools_googleSearch ?? false,
      tools_code_execution: c.tools_code_execution ?? false,
    });
  } else if (provider === "gca") {
    const c = settings.gcaConfig || {};
    return invoke("chat_gca", {
      messages: msgs,
      system_prompt: systemPrompt,
      model: settings.gcaModel || "gemini-2.5-flash",
      temperature: c.temperature ?? 0.9,
      max_tokens: c.max_tokens ?? 8192,
      top_p: c.top_p ?? null,
      top_k: c.top_k ?? null,
      frequency_penalty: c.frequency_penalty ?? null,
      presence_penalty: c.presence_penalty ?? null,
      thinking_level: c.thinking_level ?? null,
      tools_google_search: c.tools_google_search ?? false,
      tools_google_maps: c.tools_googleMaps ?? false,
      tools_url_context: c.tools_url_context ?? false,
      tools_code_execution: c.tools_code_execution ?? false,
    });
  } else if (provider === "mistral") {
    const c = settings.mistralConfig || {};
    return invoke("chat_mistral", {
      messages: msgs,
      system_prompt: systemPrompt,
      model: settings.mistralModel || "mistral-small-2603",
      api_key: settings.mistralKey || "",
      temperature: c.temperature ?? 0.9,
      max_tokens: c.max_tokens ?? 8192,
      top_p: c.top_p ?? null,
      frequency_penalty: c.frequency_penalty ?? null,
      presence_penalty: c.presence_penalty ?? null,
      reasoning_effort: c.reasoning_effort ?? null,
    });
  }
  return undefined;
}
