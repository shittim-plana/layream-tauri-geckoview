# Layream

Prompt editor, AI testing studio, and more — powered by Rust.

## Overview

- **Android app**: Tauri 2.0 + Rust
- **Web app**: Svelte + Rust (WASM)
- **Core**: Shared Rust crate for prompt parsing, AI API integration, and more

## Features (planned)

- Load/edit prompt presets (RisuAI .risup compatible)
- Test prompts with Vertex AI (OAuth) and Gemini Code Assistant
- HyPA v3 long-term memory (summarization + embedding + retrieval)
- CBS template engine support
- Background API response handling with notifications

## Tech Stack

- **Rust** — core logic, compile-time safety
- **Tauri 2.0** — native Android app
- **Svelte** — web frontend
- **Vertex AI OAuth** — default authentication

## License

See [LICENSE](LICENSE).
