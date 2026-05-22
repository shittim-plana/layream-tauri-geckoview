# Layream

Prompt editor, AI testing studio, and bot creation tool.

프롬프트 편집기, AI 테스팅 스튜디오, 봇 제작 도구.

## Overview / 개요

- **Android app**: Tauri 2.0 + Rust backend + Svelte 5 frontend + **GeckoView 128 ESR**
- **Core**: Shared Rust crate (`layream-core`) — prompt parsing, CBS evaluation, AI API, cryptography

## Features / 기능

### Prompt & Character / 프롬프트 & 캐릭터

- Load/edit prompt presets (RisuAI `.risup` / `.json` compatible, lossless roundtrip)
- **assemblePrompt** — promptTemplate traversal + CBS evaluation + regex + type-based assembly (plain/jailbreak/cot/description/persona/lorebook/authornote/postEverything/chat/memory)
- Character card viewer/editor (`.charx` / `.jpeg` / `.png` / `.json`) with lazy loading, asset gallery, alternate greetings
- `.risum` module parser (binary container + rpack + JSON)
- **Multi-module loading** — batch load + lorebook/regex/toggle merge + activation UI
- CBS template editor with syntax highlighting (Material Palenight), block diagnostics, 40+ functions
- Customscript regex editor with live testing
- Library system — save/load/delete presets, characters, modules independently

### Persona / 페르소나

- Persona list — create, edit, delete, select per session
- Import from character card
- Injected into prompt via assemblePrompt

### Workspace / 워크스페이스

- Multiple workspaces — create, switch, delete, each with own session
- Workspace selector in app header
- Per-workspace session/HyPA persistence

### Chat & AI / 채팅 & AI

- Chat with streaming (SSE) via Vertex AI, GCA, Mistral AI
- **Message editing** — inline edit/save/cancel for sent messages
- **Response swipe** — cycle through alternative responses with stable ordering
- Retry with exponential backoff + cancel token (429/5xx handling)
- First message + alternate greetings swipe
- Message delete + response regeneration + pin
- Conversation forking — branch/merge model per message
- Trigger scripts — 11 event types, condition/effect system
- **Autopilot v2** — automated conversation testing: user persona, char-to-char mode, pause/resume FSM, structured output
- **HyPA v3** — auto-summarize + cosine search + RAG context injection + pin/invalidation/cleanup + import (RisuAI hypaV3 format) + viewer modal
- Structured output (Vertex/GCA `responseSchema` + Mistral `json_schema`)

### GeckoView (Firefox Engine)

- Embedded GeckoView 128 ESR — uses Firefox rendering engine instead of Android System WebView
- Local HTTP asset server for frontend serving
- IPC via WebExtension native messaging (`cloneInto`/`exportFunction` bridge)
- OAuth via GeckoView — not subject to embedded WebView restrictions

### OAuth & Connectivity / OAuth & 연결

- Vertex AI OAuth — PKCE, deep link redirect, no client_secret
- GCA OAuth — client_secret, loopback TCP redirect
- Browser picker + Chrome Custom Tabs
- Session/preset/character persistence with app-close flush

## Supported API Providers / 지원 API

| Provider | Auth | Streaming | Embedding | Dynamic Model List |
|----------|------|-----------|-----------|-------------------|
| Vertex AI (Gemini) | OAuth (PKCE) | SSE | gemini-embedding-001/2 | /v1/publishers/google/models |
| GCA | OAuth (secret) | SSE | - | Fixed list |
| Mistral AI | API Key | SSE | - | /v1/models (capabilities filter) |
| Voyage AI | API Key | - | voyage-3 | - |

## Tech Stack / 기술 스택

- **Rust** (~10,000 LOC) — core library + Tauri backend
- **Tauri 2.0** — Android app, Rust backend
- **GeckoView 128 ESR** — embedded Firefox engine
- **Svelte 5** — frontend (~72KB gzipped), 13 lib modules
- **reqwest + rustls** — HTTP client

## Project Structure / 프로젝트 구조

```text
layream-core/       Shared Rust library (18 modules, 144 tests)
  src/
    cbs/              CBS parser + highlighter
    preset.rs         Preset parsing (RPack → gzip → msgpack → AES-GCM)
    charx.rs          Character parsing + lazy loading
    vertex_auth.rs    Vertex OAuth + PKCE
    vertex_api.rs     Vertex AI API (stream, embed, list_models)
    gca.rs            GCA API (separate OAuth, loopback)
    mistral.rs        Mistral API (chat, list_models, capabilities filter)
    retry.rs          Retry + cancel token (exponential backoff)
    hypa.rs           HyPA v3 memory engine

layream-app/        Tauri 2.0 app
  src/views/          ChatView, PresetView, CharacterView, PersonaView,
                      SettingsView, AutopilotView, HypaView, LibraryView,
                      ModuleEditView, TestView
  src/lib/            13 modules (assemblePrompt, messageStore, triggerEngine, etc.)
  src/components/     FileImport, CBSEditor, HypaModal, WorkspaceSelector, ResizableTextarea
  src-tauri/src/      commands.rs, commands_hypa.rs, persistence.rs
  gen/android/        GeckoView integration, AssetServer, IPC extension
```

## Build / 빌드

```bash
# Core tests / 코어 테스트
cargo test -p layream-core

# Frontend dev / 프론트엔드 개발
cd layream-app && npm install && npm run dev

# Android APK (GeckoView included)
source scripts/env.sh
cd layream-app
npm run tauri android build -- --apk --target aarch64
```

## Status / 상태

**v0.3.5-alpha** — [Download APK / APK 다운로드](https://github.com/shittim-plana/layream/releases/tag/v0.3.5-alpha)

144 tests (122 unit + 12 interop + 10 quality gate). APK ~184MB (GeckoView 포함).

## License / 라이선스

See [LICENSE](LICENSE).
