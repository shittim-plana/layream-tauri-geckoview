# Layream

Prompt editor, AI testing studio, and more — powered by Rust.

프롬프트 편집기, AI 테스팅 스튜디오 — Rust 기반.

## Overview / 개요

- **Android app**: Tauri 2.0 + Rust
- **Web app**: Svelte + Rust (WASM)
- **Core**: Shared Rust crate for prompt parsing, AI API integration, and more

## Features / 기능

- Load/edit prompt presets (RisuAI .risup / .json compatible)
- Character card viewer (.charx / .jpeg / .png / .json)
- CBS template editor with syntax highlighting and diagnostics
- Test prompts via Vertex AI, GCA, Mistral AI (SSE streaming)
- HyPA v3 long-term memory (summarization + embedding + retrieval)
- Customscript regex editor

---

- 프롬프트 프리셋 로드/편집 (RisuAI .risup / .json 호환)
- 캐릭터 카드 뷰어 (.charx / .jpeg / .png / .json)
- CBS 템플릿 에디터 (구문 하이라이팅 + 블록 진단)
- Vertex AI, GCA, Mistral AI로 프롬프트 테스트 (SSE 스트리밍)
- HyPA v3 장기 메모리 (요약 + 임베딩 + 검색)
- customscript 정규식 편집기

## Supported API Providers / 지원 API

| Provider | Auth | Streaming |
|----------|------|-----------|
| Vertex AI (Gemini) | OAuth | SSE |
| GCA (Gemini Code Assistant) | OAuth | SSE |
| Mistral AI | API Key | SSE |
| Voyage AI (embeddings) | API Key | - |

## Tech Stack / 기술 스택

- **Rust** — core logic, compile-time safety / 코어 로직, 컴파일 타임 안전성
- **Tauri 2.0** — native Android app / 네이티브 Android 앱
- **Svelte 5** — web frontend / 웹 프론트엔드
- **reqwest + rustls** — HTTP without system OpenSSL

## Project Structure / 프로젝트 구조

```
layream-core/     Shared Rust library (16 modules, 65 tests)
layream-app/      Tauri 2.0 app (Svelte frontend + Rust backend)
```

## Build / 빌드

```bash
# Core library / 코어 라이브러리
cargo test -p layream-core

# Frontend / 프론트엔드
cd layream-app && npm install && npm run build
```

## Status / 상태

Work in progress. Core library and app scaffold are complete. APK and web builds are not yet available.

개발 진행 중. 코어 라이브러리와 앱 골격은 완성. APK 및 웹 빌드는 아직 제공되지 않음.

## License / 라이선스

See [LICENSE](LICENSE).
