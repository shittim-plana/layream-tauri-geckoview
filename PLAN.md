# Layream — 구현 계획 (Rust)

**CLAUDE.md를 반드시 전부 읽고 모든 지침을 따를 것.**

## 프로젝트 목적
프롬프트 편집/테스트/HyPA 메모리 관리를 하나의 모바일 앱에서.
프롬프트 엔지니어링 및 봇 제작 도구 — AI 채팅 프론트엔드가 아님.
arona-bot에 넣을 프롬프트를 로컬에서 테스트하는 용도.

**플랫폼**: Tauri 2.0 Android 앱 (Rust 백엔드 + Svelte WebView)
Tauri 선택 이유: Rust 백엔드가 new-arona-bot-mk-2의 Node.js 역할을 대체. 봇 카드/모듈의 CSS 렌더링이 필요하므로 WebView 기반이 현재 올바름.
장기적으로 arona-bot 자체 포맷 전환 시 egui 네이티브 마이그레이션 가능.

## 레포지토리
- `shittim-plana/layream` — https://github.com/shittim-plana/layream
- `RisuExtractUtil` — TS 프로토타입 (UI/UX 참조)

## 참조 소스 — 반드시 확인할 것

| 소스 | 경로 | 용도 |
|------|------|------|
| RisuExtractUtil | `/config/workspace/RisuExtractUtil/` | 프로토타입 UI/UX, 파일 로딩, CBS 에디터, 테스트 파일 |
| new-arona-bot-mk-2 | `/config/workspace/new-arona-bot-mk-2/` | Vertex AI OAuth, GeminiProvider, VertexAIConnection UI |
| vertex-ai-oauth | `/config/workspace/vertex-ai-oauth/` | OAuth 서버 라이브러리, vertex_auth.rs 원본 |
| RisuAI | `/config/workspace/RisuAI/` | CBS 파서, HyPA v3 알고리즘 |
| Risu-GCA 플러그인 | `/config/workspace/RisuExtractUtil/risu-gca.js` | GCA 모델, client ID, opt-out, tools, UI |

## 아키텍처

```
layream/
├── layream-core/     Rust 코어 (16모듈, 83테스트) — ✅ 안정, 변경 없음
├── layream-app/      Tauri 2.0 앱
│   ├── src/          Svelte 5 프론트엔드 — ✅ 전면 재작성 완료
│   └── src-tauri/    Rust 백엔드 + IPC — ✅ API 커맨드 구현 완료
└── scripts/          빌드 환경 스크립트
```

## 완료된 작업 (v0.2.0-alpha)

### 프론트엔드
- ✅ CSS 테마 (프로토타입 기반, 보라색 accent)
- ✅ 하단 네비바 (4탭: Preset/Character/Test/Settings)
- ✅ FileImport 컴포넌트 (dialog 플러그인 + HTML input 폴백)
- ✅ CBSEditor 컴포넌트 (구문 하이라이팅 + 진단)
- ✅ PresetView (3 서브탭: Prompts/Regex/Parameters)
- ✅ CharacterView (4 서브탭: Info/Lorebook/Assets/Module)
- ✅ TestView (5 서브탭: Chat/Autopilot/HyPA/Preview/Logs)
- ✅ SettingsView (프로바이더별 독립 카드 + 용도별 선택 + backup/restore)
- ✅ 확장자 검증 (Preset/Character 각각)
- ✅ 모바일 Enter 줄바꿈 지원

### 백엔드
- ✅ chat_vertex / chat_gca / chat_mistral (완전 분리, SSE event emit)
- ✅ embed_vertex / embed_voyage
- ✅ gca_load_code_assist / gca_check_opt_out
- ✅ highlight_cbs / evaluate_cbs
- ✅ 상태 영속화 (tokens AES-256-GCM, settings JSON, HyPA JSON)
- ✅ 토큰 리프레시 (get_valid_token 자동)
- ✅ Request/Response 로깅

## 코드 리뷰 + 버그 수정 완료 (2026-05-03, 20건)

멀티 에이전트 코드 리뷰로 21건 발견, 20건 수정 완료.
- invoke 파라미터 전수 대조 (28개) PASS
- 파일 로딩 흐름 추적 PASS (serde rename 일치)
- OAuth 흐름 + 플러그인 설정 PASS
- cargo test 85개 PASS, npm build 0 errors

### 미해결
- 오토파일럿 실행 로직 (UI만 존재)
- HyPA 자동 요약 (설정만, 실행 로직 미구현)
- 대용량 파일 IPC 최적화 (Array.from → base64 or 파일 경로)
- UI/UX 개선 (Figma 디자인 적용)

## API 프로바이더

### Vertex AI OAuth
- Android OAuth client ID: `317210024447-v4g6e0e1q5933vogajp0651vhkrgal06.apps.googleusercontent.com`
- PKCE 방식 (client_secret 없음)
- Deeplink redirect: `com.shittimplana.layream://oauth/callback`
- 임베딩 지원: gemini-embedding-001, gemini-embedding-2
- Tools: GoogleSearch, CodeExecution
- Thinking: Budget 모드

### GCA (Gemini Code Assist) — Vertex AI와 완전 별개
- Client ID: `681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com`
- Project ID 불필요 — loadCodeAssist로 자동 획득
- 별도 OAuth, 별도 Google 계정 가능
- 자동 opt-out (freeTierDataCollectionOptin)
- Tools: google_search, googleMaps, url_context, code_execution
- Thinking: Level 모드 (none/low/medium/high)
- 모델 목록: risu-gca.js 참조
- UI 레퍼런스: risu-gca.js 플러그인

### Mistral AI
- API Key 인증
- safe_prompt 파라미터 없음 (제거됨)
- reasoning_effort 지원

### Voyage AI (임베딩 전용)
- API Key 인증
- 기본 모델: voyage-4-large

## Android 빌드 (클린 빌드 필수)

```bash
sudo apt-get install -y -qq gcc libxml2 xz-utils unzip
source scripts/env.sh
cd layream-app
rm -rf ../target/aarch64-linux-android/release/deps/liblayream_app*
rm -rf ../target/aarch64-linux-android/release/liblayream_app*
rm -rf src-tauri/gen/android/app/build
npm run tauri android build -- --apk --target aarch64
```

## 핵심 규칙

- **CLAUDE.md의 모든 지침을 따를 것**
- **참조 소스 없이 추측 금지** — 모델명, API 구조, UI 패턴
- **코어(layream-core)는 건드리지 않아도 됨**
- **Vertex AI OAuth와 GCA는 완전 별개** — 인증, 계정, 설정, 모델, tools, thinking 전부 독립
- **GCA UI는 risu-gca.js 플러그인 레퍼런스**
- **빌드 시 반드시 Cargo + Gradle 캐시 삭제** — 안 하면 이전 코드가 APK에 포함됨
- **isTauri는 동적 함수** — 정적 변수로 하면 모듈 로드 시점에 false로 고정됨
