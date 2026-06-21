# Layream

> **Archived** — v0.4.2 final. 새 방향으로 재설계 예정.

프롬프트 편집기, AI 테스팅 스튜디오, 봇 제작 도구 (Android).

## Stack

- **Tauri 2.0** + Rust backend + Svelte 5 frontend
- **GeckoView 128 ESR** — embedded Firefox engine ([template](https://github.com/shittim-plana/tauri-geckoview-template))
- **layream-core** — shared Rust crate

## Features

### Prompt & Character

- Prompt presets (`.risup` / `.json`), character cards (`.charx` / `.jpeg` / `.png`), modules (`.risum`), persona cards (`.png` tEXt)
- CBS template language — logos tokenizer, LALRPOP math grammar, ~60 functions, `#escape`/`#code`/`#func` blocks, syntax highlighting + markdown
- Prompt assembly — CBS + regex (`/flags`, JS substitution) + type dispatch
- Lorebook keyword matching, multi-module loading, regex editor

### Chat & AI

- Streaming (SSE) — Vertex AI (OAuth PKCE), GCA (OAuth), Mistral AI (API Key)
- Message editing, response swipe, regeneration, conversation forking
- Trigger scripts (11 event types), autopilot (char-to-char, structured output)
- Retry + exponential backoff + cancel token

### HyPA Memory

- 4-phase selection: important → pin → recent → similar (cosine + RRF) → random
- `isImportant` (per-summary) ≠ `pinBoost` (per-message)
- Embedding: Vertex AI (gemini-embedding), Voyage AI (voyage-3)

### GeckoView

- Embedded Firefox engine — not Android System WebView
- Local HTTP asset server, WebExtension IPC
- Poll-based streaming (`poll_stream_chunks`)
- OAuth works — GeckoView is a real browser

### Workspace & Persistence

- Multiple workspaces with per-workspace session/HyPA
- JSONL request/response log (optional)
- Autosave with flush-on-switch

## API Providers

| Provider | Auth | Streaming | Embedding | Dynamic Models |
|----------|------|-----------|-----------|----------------|
| Vertex AI | OAuth (PKCE) | SSE | gemini-embedding-001/2 | Yes |
| GCA | OAuth (secret) | SSE | — | Fixed list |
| Mistral AI | API Key | SSE | — | Yes |
| Voyage AI | API Key | — | voyage-3 | — |

## Structure

```
layream-core/       Shared Rust library
  src/cbs/            CBS (logos + LALRPOP + eval + highlighter)
  src/preset.rs       Preset parsing (RPack → gzip → msgpack → AES-GCM)
  src/charx.rs        Character/persona parsing
  src/hypa.rs         HyPA memory engine
  src/regex.rs        Regex (/flags + JS substitution + script_type)
  src/vertex_auth.rs  OAuth 2.0 + PKCE
  src/vertex_api.rs   Vertex AI API

layream-app/        Tauri 2.0 Android app
  src/views/          10 views
  src/lib/            13 modules
  src-tauri/src/      8 command modules + persistence
  gen/android/        GeckoView, AssetServer, IPC
```

## Build

```bash
cargo test -p layream-core          # 273 tests
source scripts/env.sh
cd layream-app && npm install
npm run tauri android build -- --apk --target aarch64
```

## Release

**[v0.4.2-alpha](https://github.com/shittim-plana/layream-tauri-geckoview/releases/tag/v0.4.2)** (Final) — APK ~184MB, arm64, 273 tests.

## Related

- [tauri-geckoview-template](https://github.com/shittim-plana/tauri-geckoview-template) — GeckoView + Tauri 2.0 template
- [vertex-ai-oauth](https://github.com/shittim-plana/vertex-ai-oauth) — OAuth 2.0 for Vertex AI (Browser / Server / Native)

## License

See [LICENSE](LICENSE).
