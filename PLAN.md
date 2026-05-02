# Layream — 구현 계획 (Rust)

## 프로젝트 목적
프롬프트를 로드/편집하고, Vertex AI로 테스트하는 모바일 퍼스트 앱.
**두 플랫폼**: 네이티브 Android 앱 (Tauri 2.0 + Rust) + 웹 앱 (Svelte + Rust WASM)

## 레포지토리
- **레포**: `shittim-plana/layream` — https://github.com/shittim-plana/layream
- 앱 이름: **Layream**
- `RisuExtractUtil`은 TS 프로토타입으로 유지 (참조용)

## 라이선스
- **RisuAI(MIT) 의무 없음** — Rust로 clean-room 구현이므로 TS 코드 복사 없음
- 알고리즘(RPack, CBS 문법, .risup 포맷)은 저작권 대상 아님
- 프로젝트 라이선스: Attribution + No-Sell + Share-Alike (vertex-ai-oauth와 동일)

## 아키텍처

```
layream/
├── Cargo.toml                     # workspace root
├── layream-core/                  # 공유 코어 라이브러리
│   └── src/
│       ├── types.rs               # RisuAI 타입 (BotPreset, CharacterCard 등)
│       ├── crypto.rs              # AES-256-GCM + SHA-256
│       ├── rpack.rs               # RPack 순수 Rust (WASM 역공학 테이블)
│       ├── preset.rs              # .risup/.json 프리셋 파이프라인
│       ├── regex.rs               # customscript 정규식
│       ├── vertex_auth.rs         # OAuth Auth Code flow + 토큰 관리
│       ├── vertex_api.rs          # Gemini SSE 스트리밍 + thinking config
│       ├── voyage.rs              # Voyage AI 임베딩 + rerank + cosine sim
│       ├── gca.rs                 # Gemini Code Assistant API
│       ├── charx.rs               # .charx/.png/.json 캐릭터 카드
│       ├── hypa.rs                # HyPA v3 LTM (4단계 메모리 선택)
│       └── cbs/
│           ├── parser.rs          # CBS 파서 (40+ 함수, RPN)
│           └── highlighter.rs     # CBS 하이라이팅 + 진단
│
├── layream-app/                   # Tauri 2.0 앱
│   ├── src/                       # Svelte 5 프론트엔드
│   │   ├── App.svelte
│   │   └── views/                 # Preset, Character, Test, Settings
│   └── src-tauri/
│       └── src/
│           ├── lib.rs             # Tauri 진입점
│           └── commands.rs        # IPC 커맨드 (7개)
│
└── layream-web/                   # (미구현) Svelte + WASM 웹 앱
```

### 핵심 원칙
- **코어 로직은 전부 Rust** — 컴파일 타임에 에러 잡기
- **코어 crate 하나로 앱/웹 공유** — cargo 빌드 (앱) + wasm-pack (웹)
- **vertex-ai-oauth를 범용 Rust crate로** — crates.io 공개 가능 목표
- **앱은 자체 백엔드** — 토큰 관리, API 호출, 백그라운드 응답 수신

## 구현 상태 (2026-05-02)

### Phase 1 — 코어 Rust crate ✅
1. ✅ cargo workspace 구조 생성
2. ✅ types.rs — RisuAI 타입 정의 (botPreset, Message, PromptItem, CharacterCard 등)
3. ✅ crypto.rs — AES-256-GCM + SHA-256
4. ✅ rpack.rs — RPack 순수 Rust (WASM에서 256바이트 테이블 역공학)
5. ✅ preset.rs — 프리셋 로드/내보내기 (RPack→gzip→msgpack→AES 파이프라인)
6. ✅ regex.rs — customscript 정규식

### Phase 2 — 인증 + API ✅
7. ✅ vertex_auth.rs — OAuth Auth Code flow + 토큰 갱신 (5분 전 자동)
8. ✅ vertex_api.rs — Gemini API SSE 스트리밍 + 모델별 thinking config
9. ✅ voyage.rs — Voyage AI 임베딩 + rerank + cosine similarity
10. ✅ gca.rs — Gemini Code Assistant (cloudcode-pa.googleapis.com/v1internal)

### Phase 3 — 고급 기능 ✅
11. ✅ cbs/parser.rs — CBS 파서 (40+ 함수, RPN, 중첩, 변수)
12. ✅ cbs/highlighter.rs — CBS 하이라이팅 (depth 컬러, 진단)
13. ✅ charx.rs — 캐릭터 카드 파싱 (.charx ZIP, .png tEXt, RCC 암호화)
14. ✅ hypa.rs — HyPA v3 LTM (important→recent→similar→random + RRF k=60)

### Phase 4 — 앱 + 웹 (부분 완료)
15. ✅ Tauri 2.0 앱 IPC 커맨드 (7개)
16. ⬜ Android 백그라운드 서비스 + 알림
17. ✅ Svelte 5 프론트엔드 (4개 뷰: Preset, Character, Test, Settings)
18. ⬜ Svelte + WASM 웹 앱

### 테스트 현황
- **60개 단위 테스트 전부 통과**
- 실제 .risup 파일 상호운용성 테스트 포함 (마마젬 v1.26.11)
- RPack 테이블 roundtrip 검증 (256바이트 전체)
- AES-GCM roundtrip, CBS 수학/변수/비교/논리 연산

## 남은 작업

### Phase 4 잔여
- Android 백그라운드 서비스 (background.rs) + 알림 (notification.rs)
- Svelte + WASM 웹 앱 (layream-web, wasm-pack)
- scripts/setup-android-env.sh (aarch64 빌드 환경)

### Phase 5 — 포맷 독립화
- CBS 템플릿 엔진 → 자체 템플릿 엔진으로 교체
- .risup 포맷 → 구버전 호환 전용 (레거시), 자체 포맷 도입
- RisuAI 종속성 완전 제거

### Phase 6 — new-arona-bot-mk2 통합
- arona-bot 전용 버전으로 확장
- 채팅 인덱스, RAG, Voyage AI rerank 통합
- 프롬프트 테스트 → 실제 봇 운용까지 연결

### vertex-ai-oauth Rust crate 공개
- crates.io에 범용 Vertex AI OAuth crate 등록
- Auth Code flow + 토큰 관리 + SSE 스트리밍

## Vertex AI OAuth 참조 (arona-bot-mk2)

### 구현 패턴
- **Auth Code flow**: 사용자 → Google 로그인 → code → exchangeCodeForTokens() → access_token + refresh_token
- **토큰 관리**: 메모리 캐시 + 영구 저장, 만료 5분 전 자동 갱신
- **API 호출**: POST streamGenerateContent, SSE 파싱 (data: {...} 라인)
- **에러 처리**: invalid_grant → 재인증 (refresh_token 50개 제한, 6개월 미사용 만료)
- **모델별 thinking**: gemma/gemini-3.* → HIGH, gemini-2.0-flash → disabled, gemini-3.1-* → 없음
- **Voyage AI**: 2단계 RAG — embed (cosine) → rerank

### 앱 vs 웹 OAuth 차이
- 웹: GIS popup flow (브라우저 제약)
- 앱: Auth Code flow 직접 구현 — 시스템 브라우저 → custom scheme redirect → 토큰 전달

## Android 앱 필요 권한
- `INTERNET` — API 호출
- `READ_EXTERNAL_STORAGE` (≤32) / `READ_MEDIA_*` (≥33) — 파일 불러오기
- `WRITE_EXTERNAL_STORAGE` (≤29) — 파일 내보내기
- `POST_NOTIFICATIONS` — 응답 완료 알림 (Android 13+)
- `FOREGROUND_SERVICE` + `FOREGROUND_SERVICE_DATA_SYNC` — 백그라운드 API 응답
- `WAKE_LOCK` — CPU 절전 방지

## 로컬 APK 빌드 환경 (aarch64 VSCode OSS)
- `scripts/setup-android-env.sh` 실행 (Java 17 + SDK + NDK + clang-18)
- NDK x86_64 clang → 시스템 clang-18 래퍼 (`--sysroot` 명시 필수)
- NDK 런타임 라이브러리(libunwind 등) → 시스템 clang 리소스 디렉토리 링크
- aapt2: apt aarch64 네이티브 (29.0.3), compileSdk=34 전용
- `--target aarch64` 필수, gradle.properties에 `targetList=aarch64`
- GitHub Actions: 웹 배포(Pages)만

## 핵심 제약
- **기본 인증은 Vertex AI OAuth** (GCA는 별도 추가 옵션)
- **RisuAI 프리셋이 기본** — botPreset 타입 기준
- **RisuAI Message 형식** — `role: 'user'|'char'`, `data`, `time?`, `chatId?`, `isPinned?`
- **isPinned만 Arona bot에서 가져옴** (HyPA 검색 부스트, UX 접근성)
- **임베딩 기본은 Voyage AI** (voyage-4-large)
- **preview 모델은 global 리전**
- **사용자 제약을 가정으로 덮어쓰지 말 것**

## 모델 목록

### Vertex AI OAuth
- gemini-2.5-flash, gemini-2.5-pro
- gemini-3.0-flash-preview (global), gemini-3.0-pro-preview (global)
- gemini-3.1-flash-lite-preview (global), gemini-3.1-pro-preview (global)
- gemma-4-31b-it, gemma-4-26b-a4b-it
- preview 모델 선택 시 자동 global 리전
- 커스텀 모델 직접 입력 가능

### GCA (참조: risu-gca.js 플러그인)
- gemini-2.5-flash, gemini-2.5-flash-lite, gemini-2.5-pro
- gemini-3-flash-preview, gemini-3-pro-preview
- gemini-3.1-flash-lite-preview, gemini-3.1-pro, gemini-3.1-pro-preview
- 엔드포인트: cloudcode-pa.googleapis.com/v1internal

## 관련 레포
- `shittim-plana/RisuAI` — HyPA v3 알고리즘 참조, RPack 알고리즘 참조 (코드 아닌 알고리즘만)
- `shittim-plana/vertex-ai-oauth` — Rust crate 후보, 커스텀 라이선스
- `/config/workspace/new-arona-bot-mk-2` — Vertex AI OAuth 풀스택 참조, RAG, Voyage AI
- `shittim-plana/risuai-cbs-editor` — CBS 문법 참조
- `kangjoseph90/Risu-GCA` — GCA API 참조 (risu-gca.js)
- `shittim-plana/RisuExtractUtil` — TS 프로토타입 (참조용 유지)
