# Layream

Prompt editor, AI testing studio, and more — powered by Rust.

프롬프트 편집기, AI 테스팅 스튜디오 — Rust 기반.

## Overview / 개요

- **Android app**: Tauri 2.0 + Rust
- **Web app**: Svelte + Rust (WASM)
- **Core**: Shared Rust crate for prompt parsing, AI API integration, and more

## Features / 기능

- Load/edit prompt presets (RisuAI .risup / .json compatible)
- Character card viewer (.charx / .jpeg / .png / .json) with asset gallery, regex display, alternate greetings
- .risum module parser and viewer
- CBS template editor with syntax highlighting and diagnostics
- Test prompts via Vertex AI, GCA, Mistral AI (SSE streaming)
- **Autopilot v2**: automated conversation testing — user persona, char-to-char mode, pause/resume FSM, structured output (response_schema)
- **HyPA v3**: auto-summarize + cosine search + RAG context injection + Pin (pin_boost) + invalidation/cleanup + viewer modal
- Customscript regex editor
- Session/preset persistence with app-close flush
- Structured output support (Vertex/GCA responseSchema + Mistral json_schema)

---

- 프롬프트 프리셋 로드/편집 (RisuAI .risup / .json 호환)
- 캐릭터 카드 뷰어 (.charx / .jpeg / .png / .json) + 에셋 갤러리, 정규식, 대체 인사말
- .risum 모듈 파서 및 뷰어
- CBS 템플릿 에디터 (구문 하이라이팅 + 블록 진단)
- Vertex AI, GCA, Mistral AI로 프롬프트 테스트 (SSE 스트리밍)
- **오토파일럿 v2**: 자동 대화 테스트 — 유저 페르소나, 캐릭터 간 대화, 일시정지/재개, structured output
- **HyPA v3**: 자동 요약 + cosine 검색 + RAG 컨텍스트 주입 + 핀(pin_boost) + 무효화/정리 + 뷰어 모달
- customscript 정규식 편집기
- 세션/프리셋 영속화 + 앱 종료 시 자동 저장
- Structured output (Vertex/GCA responseSchema + Mistral json_schema)

## Supported API Providers / 지원 API

| Provider | Auth | Streaming | Dynamic Model List |
|----------|------|-----------|-------------------|
| Vertex AI (Gemini) | OAuth | SSE | /v1/publishers/google/models |
| GCA (Gemini Code Assistant) | OAuth | SSE | - |
| Mistral AI | API Key | SSE | /v1/models |
| Voyage AI (embeddings) | API Key | - | - |

Model selection supports both predefined suggestions and free-text input for unlisted/hidden models.

모델 선택은 고정 제안 목록 + 자유 입력 + API 동적 조회를 지원. 비공개 모델도 직접 입력 가능.

## Tech Stack / 기술 스택

- **Rust** — core logic, compile-time safety / 코어 로직, 컴파일 타임 안전성
- **Tauri 2.0** — native Android app / 네이티브 Android 앱
- **Svelte 5** — web frontend / 웹 프론트엔드
- **reqwest + rustls** — HTTP without system OpenSSL

## Project Structure / 프로젝트 구조

```
layream-core/     Shared Rust library (16 modules, 85+ tests)
layream-app/      Tauri 2.0 app (Svelte 5 frontend + Rust backend)
  src/views/        ChatView, AutopilotView, HypaView, TestView, PresetView, CharacterView, SettingsView
  src/components/   FileImport, CBSEditor, HypaModal
  src-tauri/src/    commands.rs, commands_hypa.rs, persistence.rs, lib.rs
```

## Build / 빌드

```bash
# Core library / 코어 라이브러리
cargo test -p layream-core

# Frontend / 프론트엔드
cd layream-app && npm install && npm run build
```

## Status / 상태

**v0.3.0-alpha** — [Download APK](https://github.com/shittim-plana/layream/releases/tag/v0.3.0-alpha)

Core library stable (85+ tests). Android APK available as prerelease. Web build not yet available.

**v0.3.0-alpha** — [APK 다운로드](https://github.com/shittim-plana/layream/releases/tag/v0.3.0-alpha)

코어 라이브러리 안정 (85+ 테스트). Android APK prerelease 제공. 웹 빌드는 미제공.

## License / 라이선스

See [LICENSE](LICENSE).
