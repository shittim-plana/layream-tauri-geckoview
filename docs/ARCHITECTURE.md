# Layream — Architecture

## Overview

Prompt editor, AI testing studio, and bot creation tool.
Android app: Tauri 2.0 + Rust backend + Svelte 5 frontend + GeckoView 128 ESR.

```
layream-core/       Shared Rust library — parsing, evaluation, AI API, cryptography
layream-app/        Tauri 2.0 Android app
  src-tauri/        Rust backend (7 command modules + HyPA + persistence)
  src/              Svelte 5 frontend (10 views, 13 lib modules, components)
  gen/android/      GeckoView integration, AssetServer, IPC extension
```

---

## Data Flow: Chat Request

```
1. User message input
2. assemblePrompt:
   preset.promptTemplate traversal
   → CBS evaluation (char_name, user_name, toggles)
   → regex application (preset.regex)
   → type-based dispatch:
     plain/jailbreak/cot → text
     description → character description
     persona → user persona
     chat → message position
     lorebook → keyword-matched entries
     memory → HyPA 4-phase selection (important → pin → recent → similar → random)
     authornote → author note
     postEverything → final position
     cache → explicit caching boundary
3. system_instruction = assembled result
4. contents = conversation messages
5. API call (Vertex AI / GCA / Mistral)
```

---

## CBS Template Language

CBS is a string-rewriting macro language used in prompt presets.

### Pipeline

```
Source text → Shared Tokenizer (logos) → Segments → Parser (evaluate) → Output
                                      → Highlighter (colorize) → Editor display
```

- **Tokenizer** (`tokenizer.rs`): logos lexer emits flat `{{ }}`/`}}` /text tokens; a structural grouping pass pairs balanced delimiters into `Segment`s. Single implementation (`scan_spanned`); `scan()` is a thin adapter. `#escape` block interiors captured as literal segments (never evaluated or highlighted as CBS).
- **Parser** (`parser.rs`): consumes `Segment` → `Node` tree (Text / Tag / Block). Evaluation walks the tree, dispatching ~100 functions + block constructs (#if/#when/#each/#escape/#code/#func).
- **Highlighter** (`highlighter.rs`): consumes the same tokenizer output. CBS tokens get depth-colored; text outside `{{ }}` gets markdown highlighting (headings, bold, italic, code, links). Escape regions rendered as plain text.
- **Math sub-language**: LALRPOP grammar (`grammar.lalrpop`) + logos lexer (`ast.rs::MathToken`). Handles `{{calc::...}}` and `{{?...}}` expressions.

### Block Types

| Block | Behavior |
|-------|----------|
| `#if` / `#if_pure` / `#when` | Conditional (with `:else`) |
| `#each` | Array iteration |
| `#escape` | Body is literal — `{{ }}` inside not evaluated |
| `#code` | Strip whitespace, then evaluate |
| `#func` + `call::` | Function definition and invocation |
| `#pure` / `#puredisplay` | Body passed through without evaluation |

---

## HyPA Memory System

Hierarchical Prompt Augmentation — summarize conversation history, select relevant memories for context injection.

### Selection Pipeline (`select_memories`)

```
Phase 0: Filter invalidated summaries
Phase 1: Always-include (isImportant = true)
Phase 1-pin: Guaranteed-budget pinned (pinBoost > 0) — separate from Phase 1
Phase 2: Recent (newest first, recentMemoryRatio)
Phase 3: Similar (cosine + RRF multi-query, similarMemoryRatio)
Phase 4: Random (remaining budget, shuffle)
Final: Sort selected by chronological order
```

- **isImportant** (per-summary toggle, HypaView) and **pinBoost** (per-message pin, ChatView) are independent mechanisms. Pins get a dedicated guaranteed budget — never truncated by similarity ranking.
- Token budget = `maxContext × memoryTokensRatio` − wrapper cost.
- Settings (ratios, summarization prompt) come from the imported preset.

### Two Injection Paths

| Path | Selection | Placement | Role |
|------|-----------|-----------|------|
| Memory slot | Full 4-phase select_memories | Preset-defined position | Long-term memory archive |
| ChatView [Memory] | Cosine similarity top-k | Before user message | Associative recall |

### Data Format (Interoperability)

```json
{
  "summaries": [
    { "text": "...", "chatMemos": [], "isImportant": false,
      "pinBoost": 0.0, "invalidated": false, "embedding": [...] }
  ]
}
```

Wire format: camelCase keys. `embedding` absent in external exports (recomputed on first use). Extra fields round-tripped via serde flatten.

---

## Request/Response Logs

- In-memory ring buffer (MAX_LOGS cap, volatile) + optional JSONL file persistence.
- Toggle via settings (`logPersistence` key). Default off.
- TestView UI checkbox.

---

## Backend Modules

| Module | File | Responsibility |
|--------|------|----------------|
| auth | `commands_auth.rs` | OAuth (Vertex PKCE, GCA loopback) |
| chat | `commands_chat.rs` | Chat streaming, embedding, model list, logs |
| library | `commands_library.rs` | Preset/character/module CRUD |
| workspace | `commands_workspace.rs` | Workspace CRUD |
| settings | `commands_settings.rs` | Settings, session, persona persistence |
| cbs | `commands_cbs.rs` | CBS evaluate + highlight |
| platform | `commands_platform.rs` | Android (browser, permission, notification) |
| hypa | `commands_hypa.rs` | HyPA summarize, search, select, pin, invalidate |
| persistence | `persistence.rs` | File I/O helpers |

---

## Frontend Modules

| Module | Responsibility |
|--------|----------------|
| `appStore.svelte.js` | Central store (Svelte 5 runes) |
| `assemblePrompt.js` | Prompt assembly (CBS + regex + type dispatch + memory) |
| `chatActions.js` | Send, regenerate, delete, fork |
| `messageStore.js` | Message state (branch model: parentId/branchId) |
| `streamingManager.js` | SSE streaming (poll_stream_chunks) |
| `chatSession.js` | Session serialize/restore |
| `triggerEngine.js` | Trigger script engine (11 event types) |
| `markdownRenderer.js` | Markdown → HTML (XSS safe) |
| `errors.js` | Error classification + toast |
| `flashError.js` | Toast notification system |
| `autosave.js` | Debounced auto-save |

---

## GeckoView Integration

Embedded Firefox engine (GeckoView 128 ESR). Not Android System WebView.

- **Asset serving**: localhost HTTP server serves Svelte frontend
- **IPC**: WebExtension content script → `sendNativeMessage` → Kotlin `MessageDelegate` → Rust
- **Streaming**: poll-based (`poll_stream_chunks`) — Tauri `emit`/`listen` doesn't reach GeckoView
- **OAuth**: Works because GeckoView is a real browser, not an embedded WebView

See `docs/geckoview-integration.md` for details.

---

## Supported API Providers

| Provider | Auth | Streaming | Embedding | Dynamic Model List |
|----------|------|-----------|-----------|-------------------|
| Vertex AI | OAuth (PKCE) | SSE | gemini-embedding-001/2 | /v1/publishers/google/models |
| GCA | OAuth (secret) | SSE | — | Fixed list |
| Mistral AI | API Key | SSE | — | /v1/models (capabilities filter) |
| Voyage AI | API Key | — | voyage-3 | — |

---

## Build

```bash
# Core tests
cargo test -p layream-core

# Frontend dev
cd layream-app && npm install && npm run dev

# Android APK (GeckoView included)
source scripts/env.sh
cd layream-app
npm run tauri android build -- --apk --target aarch64
```
