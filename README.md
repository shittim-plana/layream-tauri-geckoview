# Layream

Prompt editor, AI testing studio, and bot creation tool — powered by Rust.

프롬프트 편집기, AI 테스팅 스튜디오, 봇 제작 도구 — Rust 기반.

## Overview / 개요

- **Android app**: Tauri 2.0 + Rust backend + Svelte 5 frontend
- **Web app** (planned): Svelte + Rust (WASM), GitHub Pages
- **Core**: Shared Rust crate (`layream-core`) — prompt parsing, CBS evaluation, AI API, cryptography

## Features / 기능

### Prompt & Character / 프롬프트 & 캐릭터

- Load/edit prompt presets (RisuAI `.risup` / `.json` compatible, lossless roundtrip)
- **assemblePrompt** — promptTemplate traversal + CBS evaluation + regex + type-based assembly (plain/jailbreak/cot/description/persona/lorebook/authornote/postEverything/chat)
- Character card viewer (`.charx` / `.jpeg` / `.png` / `.json`) with lazy loading, asset gallery, emotion mapping, alternate greetings
- `.risum` module parser (binary container + rpack + JSON)
- CBS template editor with syntax highlighting, block diagnostics (#when/#each/#puredisplay/#if), 40+ functions
- Customscript regex editor with live testing
- Library system — save/load/delete presets, characters, modules independently

### Chat & AI / 채팅 & AI

- Chat with streaming (SSE) via Vertex AI, GCA, Mistral AI
- Retry with exponential backoff + cancel token (429/5xx handling)
- First message + alternate greetings swipe
- Message delete + response regeneration
- **Autopilot v2** — automated conversation testing: user persona, char-to-char mode, pause/resume FSM, structured output
- **HyPA v3** — auto-summarize + cosine search + RAG context injection + pin/invalidation/cleanup + import (RisuAI hypaV3 format) + viewer modal
- Structured output (Vertex/GCA `responseSchema` + Mistral `json_schema`)

### OAuth & Connectivity / OAuth & 연결

- Vertex AI OAuth — PKCE, deep link redirect, no client_secret
- GCA OAuth — client_secret, loopback TCP redirect
- Browser picker + Chrome Custom Tabs
- Session/preset/character persistence with app-close flush

---

- 프롬프트 프리셋 로드/편집 (RisuAI `.risup` / `.json` 호환, 무손실 왕복)
- **assemblePrompt** — promptTemplate 순회 + CBS 평가 + regex + type별 조립
- 캐릭터 카드 뷰어 + lazy loading, 에셋 갤러리, 감정 매핑, 대체 인사말
- `.risum` 모듈 파서 (바이너리 컨테이너 + rpack + JSON)
- CBS 템플릿 에디터 (구문 하이라이팅 + 블록 진단 + 40+ 함수)
- 라이브러리 시스템 — 프리셋/캐릭터/모듈 독립 저장/로드/삭제

## Supported API Providers / 지원 API

| Provider | Auth | Streaming | Embedding | Dynamic Model List |
|----------|------|-----------|-----------|-------------------|
| Vertex AI (Gemini) | OAuth (PKCE) | SSE | gemini-embedding-001/2 | /v1/publishers/google/models |
| GCA | OAuth (secret) | SSE | - | Fixed list |
| Mistral AI | API Key | SSE | - | /v1/models (capabilities filter) |
| Voyage AI | API Key | - | voyage-3 | - |

Model selection: predefined suggestions + free-text input + API dynamic fetch.

모델 선택: 고정 제안 + 자유 입력 + API 동적 조회.

## Tech Stack / 기술 스택

- **Rust** (5,400+ LOC) — core logic, zero production `.unwrap()`, compile-time safety
- **Tauri 2.0** — native Android app, Rust backend as server
- **Svelte 5** — web frontend (~50KB gzipped)
- **reqwest + rustls** — HTTP without system OpenSSL

## Project Structure / 프로젝트 구조

```
layream-core/       Shared Rust library (18 modules, 94 tests)
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
  src/views/          ChatView, PresetView, CharacterView, SettingsView,
                      AutopilotView, HypaView, LibraryView, TestView
  src/components/     FileImport, CBSEditor, HypaModal
  src-tauri/src/      commands.rs, commands_hypa.rs, persistence.rs
```

## Build / 빌드

```bash
# Core tests / 코어 테스트
cargo test -p layream-core

# Frontend dev / 프론트엔드 개발
cd layream-app && npm install && npm run dev

# Android APK
source scripts/env.sh
cd layream-app
npm run tauri android build -- --apk --target aarch64
```

## Status / 상태

**v0.3.1** — [Download APK](https://github.com/shittim-plana/layream/releases/tag/v0.3.1)

Core library stable (94 tests, 0 production unwrap). Soundness audit: 21 violations found, 16 fixed. Android APK available. Web build planned.

**v0.3.1** — [APK 다운로드](https://github.com/shittim-plana/layream/releases/tag/v0.3.1)

코어 라이브러리 안정 (94개 테스트, 프로덕션 unwrap 0개). Soundness 감사: 21개 위반 발견, 16개 수정. Android APK 제공. 웹 빌드 예정.

## License / 라이선스

See [LICENSE](LICENSE).
