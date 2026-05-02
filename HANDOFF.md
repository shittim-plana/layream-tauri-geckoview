# Handoff — 현재 상태 요약

## 브랜치
`main` — 모든 변경이 여기에 있음.

## 마지막 커밋
`a815f96` — Phase 1-4 전체 구현 (48파일, 24,397줄)

## 구현 완료 상태

### layream-core (공유 코어 라이브러리) — 15개 모듈, 60개 테스트
| 모듈 | 기능 | 테스트 |
|------|------|--------|
| types.rs | RisuAI 타입 (BotPreset, CharacterCard, PromptItem 등) | JSON/msgpack serde |
| crypto.rs | AES-256-GCM + SHA-256 (고정 IV 12바이트 0) | roundtrip, wrong password |
| rpack.rs | RPack 순수 Rust (WASM 역공학 256바이트 테이블) | 전체 바이트 roundtrip |
| preset.rs | .risup/.json 로드/내보내기 (RPack→gzip→msgpack→AES) | JSON/risup roundtrip + 실제 파일 |
| regex.rs | customscript 정규식 (플래그 조건부 실행) | 7개 |
| vertex_auth.rs | OAuth Auth Code flow + 토큰 갱신 5분 전 | URL 포맷, 만료 판정 |
| vertex_api.rs | Gemini SSE 스트리밍 + 모델별 thinking config | 엔드포인트, thinking 분기 6개 |
| voyage.rs | 임베딩 + rerank + cosine similarity + top-k | cosine 5개 |
| gca.rs | GCA API (cloudcode-pa.googleapis.com/v1internal) | 엔드포인트, 모델 목록 |
| charx.rs | .charx(ZIP)/.png(tEXt)/RCC/.json 캐릭터 카드 | PNG 청크, JSON 카드 |
| hypa.rs | HyPA v3 LTM (important→recent→similar→random, RRF k=60) | orphan, selection, block |
| cbs/parser.rs | CBS 파서 (40+ 함수, RPN, 변수, 중첩) | 12개 |
| cbs/highlighter.rs | CBS 하이라이팅 (depth 컬러, 블록 진단) | 7개 |

### layream-app (Tauri 2.0 앱) — Svelte 5 + Rust
| 컴포넌트 | 상태 |
|----------|------|
| Svelte 5 + Vite 6 프론트엔드 | ✅ 빌드 성공 (50KB gzipped) |
| 4개 뷰 (Preset, Character, Test, Settings) | ✅ |
| 7개 IPC 커맨드 | ✅ cargo check 통과 |
| Tauri 2.0 capabilities 설정 | ✅ |
| 다크 테마 CSS | ✅ |
| Android 백그라운드 서비스 | ⬜ 미구현 |
| 실제 Vertex AI OAuth 연동 | ⬜ stub만 |

### 상호운용성 검증
- 실제 마마젬 v1.26.11 .risup 파일 로드 + roundtrip 성공
- RPack WASM 원본과 동일한 테이블 (Node.js로 전체 256바이트 검증)
- msgpack → rmpv::Value → JSON 변환으로 TS @msgpack/msgpack 호환

## 기술 결정 기록

### RPack 역공학 (2026-05-02)
- WASM 바이너리에서 Node.js로 256바이트 encode/decode 테이블 추출
- per-byte substitution, XOR도 rotation도 아닌 임의 매핑
- roundtrip 검증: `decode(encode(i)) == i` 전체 0-255 통과

### msgpack 호환성 (2026-05-02)
- TS `@msgpack/msgpack`와 Rust `rmp-serde`의 Binary 타입 처리 차이
- `#[serde(flatten)]` + msgpack = "newtype struct" 에러 → flatten 제거
- 해결: msgpack → `rmpv::Value` → `serde_json::Value` → `BotPreset` 변환 경로

### 압축 포맷 (2026-05-02)
- TS `fflate.compressSync`/`decompressSync` = gzip (0x1f 0x8b 헤더)
- 초기 구현에서 raw deflate 사용 → 실제 파일 테스트에서 발견 → gzip으로 수정

### Tauri 2.0 설정 (2026-05-02)
- `tauri-plugin-fs` 2.5.0: `scope` 필드 제거됨 → capabilities로 교체
- 아이콘: RGBA PNG 필수 (RGB → "icon is not RGBA" 에러)
- GTK/WebKit 의존성: `apt install libgtk-3-dev libwebkit2gtk-4.1-dev` 필요 (Linux)

### TS 타입 → Rust 타입 매핑
- `botPreset.top_k`: TS에서 음수 (-1000) 가능 → `Option<i32>` (u32 아님)
- `openrouterProvider`: TS에서 String 또는 Object → `Option<Value>`
- `localStopStrings`: TS에서 `{type: 0, data: [0]}` 객체 가능 → `Option<Value>`
- `customAPIFormat`: TS enum 인덱스(숫자) → `Option<u32>`
- `customFlags`: TS enum 인덱스 배열 → `Option<Vec<u32>>`

## 남은 작업
1. Android 백그라운드 서비스 + 알림
2. Svelte + WASM 웹 앱 (layream-web)
3. scripts/setup-android-env.sh
4. 실제 OAuth 연동 (stub → 구현)
5. CBS 블록 구조 (#when, #each, #func) 완성
6. Phase 5-6 (장기)

## 빌드 환경 요구사항

### Rust 코어 (cargo check)
- rustc 1.95+
- gcc (host linker)

### Tauri 앱 (Linux)
- pkg-config, libglib2.0-dev, libgtk-3-dev
- libwebkit2gtk-4.1-dev, libjavascriptcoregtk-4.1-dev
- libayatana-appindicator3-dev, librsvg2-dev

### Android APK (aarch64 환경)
- scripts/setup-android-env.sh 참조
- NDK r27d, clang-18, compileSdk=34

### 프론트엔드
- Node.js 22+, npm
- `cd layream-app && npm install && npm run build`

## 관련 파일
- `PLAN.md` — 전체 구현 계획 + 진행 상태
- `LICENSE` — Attribution + No-Sell + Share-Alike
- `README.md` — 프로젝트 개요
