/**
 * RisuAI-compatible trigger script engine for layream.
 *
 * Reads `extensions.risuai.triggerscript` from a character card,
 * evaluates conditions against a variables map, and executes effects
 * that modify text or variables.
 *
 * Security: `runjs` / `runlua` / `runcbs` effects are intentionally
 * unsupported — a console warning is emitted instead.
 */

// ── Trigger types ────────────────────────────────────────────────────

export const TriggerType = {
  START: "start",
  INPUT: "input",
  OUTPUT: "output",
  MANUAL: "manual",
  EDIT_INPUT: "editinput",
  EDIT_OUTPUT: "editoutput",
  EDIT_PROCESS: "editprocess",
  EDIT_DISPLAY: "editdisplay",
  CHAT_COMPLETE: "chat_complete",
  INPUT_SENT: "input_sent",
  OUTPUT_RECEIVED: "output_received",
};

// ── Condition evaluation ─────────────────────────────────────────────

/**
 * Resolve a value that may reference a variable.
 * If the condition/effect declares `varRef: true` (or equivalent), look up
 * the value from the variables map; otherwise treat it as a literal.
 */
function resolveValue(raw, variables) {
  if (raw == null) return "";
  return String(raw);
}

function getVar(name, variables) {
  if (name == null) return "";
  return variables[String(name)] ?? "";
}

/**
 * Evaluate a single trigger condition.
 * @param {object} condition
 * @param {Record<string, string>} variables - session trigger variables
 * @param {object} [opts] - { chatIndex, recentMessages }
 * @returns {boolean}
 */
export function evaluateCondition(condition, variables, opts = {}) {
  if (!condition || !condition.type) return true;

  const type = condition.type;

  // "always" / "never" — trivial gates
  if (type === "always") return true;
  if (type === "never") return false;

  // Variable-based conditions: { type: "var", var: "hp", value: "10", operator: "=" }
  // Also handle the charx.ts ConditionType variants
  if (type === "var" || type === "value") {
    const actual = getVar(condition.var, variables);
    const expected = resolveValue(condition.value, variables);
    return compareValues(actual, expected, condition.operator || "=");
  }

  // Equality / inequality shortcuts from ConditionType enum
  if (type === "equal") {
    return getVar(condition.var, variables) === resolveValue(condition.value, variables);
  }
  if (type === "not_equal") {
    return getVar(condition.var, variables) !== resolveValue(condition.value, variables);
  }
  if (type === "greater") {
    return Number(getVar(condition.var, variables)) > Number(resolveValue(condition.value, variables));
  }
  if (type === "less") {
    return Number(getVar(condition.var, variables)) < Number(resolveValue(condition.value, variables));
  }

  // String containment
  if (type === "contains") {
    return getVar(condition.var, variables).includes(resolveValue(condition.value, variables));
  }
  if (type === "not_contains") {
    return !getVar(condition.var, variables).includes(resolveValue(condition.value, variables));
  }

  // Existence
  if (type === "exists") {
    // When used with depth / type2 (chat-history search)
    if (condition.type2 != null && opts.recentMessages) {
      const searchText = resolveValue(condition.value, variables);
      const depth = condition.depth || 10;
      const recent = opts.recentMessages.slice(-depth);
      if (condition.type2 === "strict") return recent.some(m => (m.content || m.text || "") === searchText);
      if (condition.type2 === "loose") return recent.some(m => (m.content || m.text || "").includes(searchText));
      if (condition.type2 === "regex") {
        try { return recent.some(m => new RegExp(searchText).test(m.content || m.text || "")); }
        catch { return false; }
      }
    }
    // Simple variable existence
    const varName = condition.var ?? condition.value;
    return varName != null && variables[String(varName)] != null && variables[String(varName)] !== "";
  }
  if (type === "not_exists" || type === "not_var") {
    const varName = condition.var ?? condition.value;
    return varName == null || variables[String(varName)] == null || variables[String(varName)] === "";
  }

  // ChatIndex condition
  if (type === "chatindex") {
    const chatIdx = opts.chatIndex ?? -1;
    return compareValues(String(chatIdx), resolveValue(condition.value, variables), condition.operator || "=");
  }

  // Unknown condition type — pass by default (permissive)
  console.warn(`[triggerEngine] Unknown condition type: ${type}`);
  return true;
}

/**
 * Compare two string values with a given operator.
 */
function compareValues(a, b, operator) {
  switch (operator) {
    case "=": case "==": return a === b;
    case "!=": return a !== b;
    case ">": return Number(a) > Number(b);
    case "<": return Number(a) < Number(b);
    case ">=": return Number(a) >= Number(b);
    case "<=": return Number(a) <= Number(b);
    case "null": return a === "" || a == null;
    case "true": return a === "1" || a === "true";
    default: return false;
  }
}

// ── Effect execution ─────────────────────────────────────────────────

/**
 * Execute a single trigger effect, mutating `context` in place.
 *
 * @param {object} effect
 * @param {object} context - { variables, text, messages, stopped }
 *   - variables: Record<string, string>  (session trigger vars, mutated)
 *   - text: string | null                (current text being processed, for edit* triggers)
 *   - messages: Array | null             (chat messages, for chat manipulation)
 *   - stopped: boolean                   (set to true by 'stop' effect)
 * @returns {void}
 */
export function executeEffect(effect, context) {
  if (!effect || !effect.type) return;

  const type = effect.type;

  // ── Variable manipulation ──────────────────────────────────────
  if (type === "setvar" || type === "v2SetVar") {
    const varName = effect.var;
    if (varName == null) return;
    const raw = effect.valueType === "var"
      ? (context.variables[effect.value] ?? "")
      : resolveValue(effect.value, context.variables);
    const op = effect.operator || "=";
    const current = context.variables[varName] ?? "";

    switch (op) {
      case "=": context.variables[varName] = raw; break;
      case "+=": context.variables[varName] = String(Number(current) + Number(raw)); break;
      case "-=": context.variables[varName] = String(Number(current) - Number(raw)); break;
      case "*=": context.variables[varName] = String(Number(current) * Number(raw)); break;
      case "/=": context.variables[varName] = String(Number(current) / Number(raw)); break;
      case "%=": context.variables[varName] = String(Number(current) % Number(raw)); break;
    }
    return;
  }

  if (type === "addvar") {
    const varName = effect.var;
    if (varName == null) return;
    const val = resolveValue(effect.value, context.variables);
    const current = context.variables[varName] ?? "";
    context.variables[varName] = String(Number(current) + Number(val));
    return;
  }

  if (type === "subvar") {
    const varName = effect.var;
    if (varName == null) return;
    const val = resolveValue(effect.value, context.variables);
    const current = context.variables[varName] ?? "";
    context.variables[varName] = String(Number(current) - Number(val));
    return;
  }

  if (type === "mulvar") {
    const varName = effect.var;
    if (varName == null) return;
    const val = resolveValue(effect.value, context.variables);
    const current = context.variables[varName] ?? "";
    context.variables[varName] = String(Number(current) * Number(val));
    return;
  }

  if (type === "divvar") {
    const varName = effect.var;
    if (varName == null) return;
    const val = resolveValue(effect.value, context.variables);
    const current = context.variables[varName] ?? "";
    context.variables[varName] = String(Number(current) / Number(val));
    return;
  }

  // ── Text effects (say / output / input) ────────────────────────
  if (type === "say") {
    // Append text to context.text (display-side injection)
    const text = resolveValue(effect.value, context.variables);
    if (text && context.text != null) {
      context.text = context.text + "\n" + text;
    }
    return;
  }

  if (type === "output") {
    // Replace context.text
    const text = resolveValue(effect.value, context.variables);
    if (text != null) context.text = text;
    return;
  }

  if (type === "input") {
    // Replace context.text (for editinput triggers)
    const text = resolveValue(effect.value, context.variables);
    if (text != null) context.text = text;
    return;
  }

  if (type === "sendas") {
    // Inject a message as a specific role — append to messages if available
    if (context.messages) {
      const role = effect.role || "char";
      const text = resolveValue(effect.value, context.variables);
      context.messages.push({ role, content: text });
    }
    return;
  }

  // ── System prompt injection ────────────────────────────────────
  if (type === "systemprompt" || type === "v2SystemPrompt") {
    const location = effect.location || "start";
    const text = effect.valueType === "var"
      ? (context.variables[effect.value] ?? "")
      : resolveValue(effect.value, context.variables);
    if (!context.systemPrompts) context.systemPrompts = {};
    const existing = context.systemPrompts[location] || "";
    context.systemPrompts[location] = existing ? existing + "\n" + text : text;
    return;
  }

  // ── Regex extraction ───────────────────────────────────────────
  if (type === "extractRegex" || type === "v2ExtractRegex") {
    const input = effect.inputVar
      ? (context.variables[effect.inputVar] ?? "")
      : (context.text ?? "");
    try {
      const regex = new RegExp(effect.regex, effect.flags || "");
      const match = regex.exec(input);
      if (match && effect.result) {
        context.variables[effect.result] = match[1] ?? match[0] ?? "";
      }
    } catch (e) {
      console.warn(`[triggerEngine] extractRegex failed:`, e);
    }
    return;
  }

  // ── Chat manipulation ──────────────────────────────────────────
  if (type === "modifychat" || type === "v2ModifyChat") {
    if (!context.messages) return;
    const index = effect.indexType === "var"
      ? Number(context.variables[effect.index] ?? 0)
      : Number(effect.index ?? 0);
    const value = effect.valueType === "var"
      ? (context.variables[effect.value] ?? "")
      : resolveValue(effect.value, context.variables);
    if (index >= 0 && index < context.messages.length) {
      const msg = context.messages[index];
      if (msg.content != null) msg.content = value;
      else if (msg.text != null) msg.text = value;
    }
    return;
  }

  if (type === "cutchat" || type === "v2CutChat") {
    if (!context.messages) return;
    const start = effect.startType === "var"
      ? Number(context.variables[effect.start] ?? 0)
      : Number(effect.start ?? 0);
    const end = effect.endType === "var"
      ? Number(context.variables[effect.end] ?? context.messages.length)
      : Number(effect.end ?? context.messages.length);
    context.messages = context.messages.slice(start, end);
    return;
  }

  // ── Control flow ───────────────────────────────────────────────
  if (type === "stop" || type === "v2StopTrigger") {
    context.stopped = true;
    return;
  }

  // ── Console log ────────────────────────────────────────────────
  if (type === "v2ConsoleLog") {
    const value = effect.sourceType === "var"
      ? (context.variables[effect.source] ?? "")
      : (effect.source ?? "");
    console.log("[triggerScript]", value);
    return;
  }

  // ── Alert ──────────────────────────────────────────────────────
  if (type === "showAlert" || type === "v2ShowAlert") {
    const text = resolveValue(effect.value, context.variables);
    console.log("[triggerScript alert]", text);
    return;
  }

  // ── Disallowed: code execution effects ─────────────────────────
  if (type === "runjs" || type === "runlua" || type === "runcbs"
      || type === "triggercode" || type === "triggerlua") {
    console.warn(`[triggerEngine] "${type}" effect is disabled for security. Skipping.`);
    return;
  }

  // ── Unknown ────────────────────────────────────────────────────
  console.warn(`[triggerEngine] Unknown effect type: ${type}`);
}

// ── Main entry point ─────────────────────────────────────────────────

/**
 * Run all triggers of a given type against a context.
 *
 * @param {object[]} triggers - array of TriggerScript objects from the card
 * @param {string} type - one of TriggerType values
 * @param {object} context - { variables, text, messages, stopped, chatIndex, recentMessages }
 * @returns {object} the (mutated) context
 */
export function runTriggers(triggers, type, context) {
  if (!Array.isArray(triggers) || triggers.length === 0) return context;

  const matching = triggers.filter(t => t && t.type === type);

  for (const trigger of matching) {
    if (context.stopped) break;

    // Evaluate all conditions (AND logic — all must pass)
    const conditions = trigger.conditions || [];
    const allPass = conditions.every(c =>
      evaluateCondition(c, context.variables, {
        chatIndex: context.chatIndex,
        recentMessages: context.recentMessages,
      })
    );
    if (!allPass) continue;

    // Execute effects in order
    const effects = trigger.effect || [];
    for (const eff of effects) {
      if (context.stopped) break;
      try {
        executeEffect(eff, context);
      } catch (e) {
        console.error(`[triggerEngine] Effect execution failed (${eff.type}):`, e);
      }
    }
  }

  return context;
}

// ── Helpers ──────────────────────────────────────────────────────────

/**
 * Extract triggerscript array from a loaded character object.
 * Handles both `card.data.extensions.risuai.triggerscript` and
 * `card.extensions.risuai.triggerscript` paths.
 *
 * @param {object|null} loadedCharacter
 * @returns {object[]}
 */
export function extractTriggerScripts(loadedCharacter) {
  if (!loadedCharacter) return [];
  const card = loadedCharacter?.card?.data || loadedCharacter?.card || {};
  const ext = card.extensions?.risuai || {};
  const scripts = ext.triggerscript;
  return Array.isArray(scripts) ? scripts : [];
}

/**
 * Create a fresh trigger context for running triggers.
 *
 * @param {Record<string, string>} variables - existing session trigger variables
 * @param {object} [opts]
 * @param {string} [opts.text] - text being processed
 * @param {object[]} [opts.messages] - chat messages
 * @param {number} [opts.chatIndex] - current chat index
 * @param {object[]} [opts.recentMessages] - recent messages for exists-condition
 * @returns {object} context
 */
export function createTriggerContext(variables, opts = {}) {
  return {
    variables: { ...variables },
    text: opts.text ?? null,
    messages: opts.messages ?? null,
    chatIndex: opts.chatIndex ?? -1,
    recentMessages: opts.recentMessages ?? null,
    systemPrompts: {},
    stopped: false,
  };
}
