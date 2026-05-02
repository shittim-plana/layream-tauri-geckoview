# Layream

Prompt editor, AI testing studio, and more — powered by Rust.

## Overview

- **Android app**: Tauri 2.0 + Rust
- **Web app**: Svelte + Rust (WASM)
- **Core**: Shared Rust crate for prompt parsing, AI API integration, and more

## Features

- Load/edit prompt presets (RisuAI .risup / .json compatible)
- Character card viewer (.charx / .png / .json)
- CBS template editor with syntax highlighting and diagnostics
- Test prompts via Vertex AI, GCA, Mistral AI (SSE streaming)
- HyPA v3 long-term memory (summarization + embedding + retrieval)
- Customscript regex editor

## Supported API Providers

| Provider | Auth | Streaming |
|----------|------|-----------|
| Vertex AI (Gemini) | OAuth | SSE |
| GCA (Gemini Code Assistant) | OAuth | SSE |
| Mistral AI | API Key | SSE |
| Voyage AI (embeddings) | API Key | - |

## Tech Stack

- **Rust** — core logic, compile-time safety
- **Tauri 2.0** — native Android app
- **Svelte 5** — web frontend
- **reqwest + rustls** — HTTP without system OpenSSL

## Project Structure

```
layream-core/     Shared Rust library (16 modules, 61 tests)
layream-app/      Tauri 2.0 app (Svelte frontend + Rust backend)
```

## Build

```bash
# Core library
cargo test -p layream-core

# Frontend
cd layream-app && npm install && npm run build
```

## Status

Work in progress. Core library and app scaffold are complete. APK and web builds are not yet available.

## License

See [LICENSE](LICENSE).
