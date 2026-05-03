# Handoff — 현재 상태 요약 (2026-05-03)

**CLAUDE.md를 반드시 전부 읽고 모든 지침을 따를 것.**
**참조 소스 없이 추측 금지 — 모델명, API 구조, UI 패턴 전부 원본 코드에서 확인.**

## 최신 커밋
- 릴리스: v0.2.0-alpha (prerelease)

## 완료된 버그 수정 (이번 세션, 20건)

### Critical
1. **invoke 파라미터 camelCase→snake_case** — 28개 invoke 호출 전수 대조 PASS
2. **deep-link 권한 누락** — capabilities/default.json에 추가
3. **GCA tools 직렬화** — GoogleMaps, UrlContext variant 추가 (vertex_api.rs)
4. **AES-GCM 고정 nonce** — 랜덤 nonce 생성 + 레거시 폴백 (crypto.rs)

### Medium
5. HARM_CATEGORY_CIVIC_INTEGRITY 추가 (commands.rs)
6. vertex_list_models AuthState + 토큰 리프레시
7. vertex_list_projects/gca_load/gca_check 토큰 리프레시 추가
8. 에러 메시지 role "char"→"error" (API 전송 방지)
9. preset.customscript→preset.regex (Rust 직렬화와 일치)
10. preset.assistantPrefill 경로 수정 (promptSettings.assistantPrefill)
11. option value={undefined}→"" (문자열 "undefined" 전송 방지)
12. applySettings 빈 문자열 허용 (!== undefined 체크)
13. RequestLogState MAX_LOGS=200 제한
14. CBSEditor $effect 중복 invoke 제거 (onMount로 교체)
15. FileImport 타임아웃 30s→5s + 이중 resolve 방지
16. HyPA 설정 영속화 (cmd_save/load_settings 연동)
17. SettingsView load-merge-save 패턴 (cross-view 키 보존)

### Low
18. 채팅 자동 스크롤 ($effect + requestAnimationFrame)
19. 디버그 메시지 정리 (에러만 표시)
20. unused gcaSt 변수 제거

## 코어 라이브러리 (layream-core) — 85개 테스트 통과

변경 사항:
- `crypto.rs`: 랜덤 nonce + 레거시 폴백
- `vertex_api.rs`: VertexTool에 GoogleMaps, UrlContext variant 추가

## 프론트엔드 구조

```
layream-app/src/
├── App.svelte              상단 헤더 + 하단 네비바 (4탭) + OAuth deeplink 콜백
├── app.css                 프로토타입 기반 CSS 테마
├── lib/tauri.js            isTauri() 동적 체크 + invoke 래퍼
├── components/
│   ├── FileImport.svelte   dialog → HTML input 폴백, 5초 취소, 에러만 표시
│   └── CBSEditor.svelte    CBS 구문 하이라이팅 (onMount 초기화, debounce 80ms)
└── views/
    ├── PresetView.svelte   3탭 (Prompts/Regex/Parameters), regex 필드, assistantPrefill 경로 수정
    ├── CharacterView.svelte 4탭 (Info/Lorebook/Assets/Module), 확장자 검증
    ├── TestView.svelte      5탭 (Chat/Autopilot/HyPA/Preview/Logs), 자동 스크롤, HyPA 영속화
    └── SettingsView.svelte  프로바이더별 독립 카드, load-merge-save, backup/restore
```

## 백엔드 커맨드 (commands.rs)

| 커맨드 | 상태 | 비고 |
|--------|------|------|
| chat_vertex | ✅ | SSE 스트리밍, 토큰 리프레시, 로깅 |
| chat_gca | ✅ | 독립 인증, level thinking, GoogleMaps/UrlContext tools |
| chat_mistral | ✅ | API Key, reasoning_effort |
| embed_vertex | ✅ | batch_embed_contents, 토큰 리프레시 |
| embed_voyage | ✅ | voyage::embed |
| gca_load_code_assist | ✅ | 세션 초기화, 토큰 리프레시 |
| gca_check_opt_out | ✅ | opt-out 확인, 토큰 리프레시 |
| highlight_cbs | ✅ | 구문 하이라이팅 + 진단 |
| evaluate_cbs | ✅ | CBS 평가 (char_name, user_name) |
| load_preset / export_preset | ✅ | .risup/.json 읽기/쓰기 |
| load_character | ✅ | .charx/.jpeg/.png/.json |
| vertex_oauth_* | ✅ | PKCE, deeplink, disconnect |
| gca_oauth_* | ✅ | 독립 OAuth |
| vertex_list_projects | ✅ | 토큰 리프레시 |
| vertex_list_models | ✅ | AuthState, 토큰 리프레시 |
| mistral_list_models | ✅ | api_key |
| cmd_save/load_settings | ✅ | JSON 영속화, load-merge-save |
| cmd_save/load_hypa | ✅ | HyPA 데이터 영속화 |
| get/clear_request_logs | ✅ | MAX_LOGS=200 |

## 검증 결과

- invoke 파라미터 전수 대조: 28개 전부 PASS
- 파일 로딩 흐름 추적: PASS (serde rename 일치 확인)
- OAuth 흐름 추적: PASS (PKCE, deep-link, 플러그인 설정)
- 플러그인 설정: capabilities/lib.rs/Cargo.toml/package.json 전부 PASS
- cargo test: 85개 PASS (81 unit + 4 interop)
- npm run build: 0 errors

## 빌드 관련 교훈

1. **Gradle 캐시 (`app/build`)** — 반드시 삭제 후 빌드
2. **Cargo 캐시 (`liblayream_app*`)** — dist/ 임베딩 때문에 삭제 필요
3. **gcc / libxml2** — 세션마다 재설치 필요
4. **isTauri** — 동적 함수 필수 (정적이면 모듈 로드 시 false)

## Vertex AI OAuth vs GCA — 완전 별개

| | Vertex AI OAuth | GCA |
|--|----------------|-----|
| Client ID | 317210024447-... | 681255809395-... |
| Project ID | 필수 (수동 입력) | 불필요 (loadCodeAssist 자동) |
| 임베딩 | ✅ (gemini-embedding-2, gemini-embedding-001) | ❌ |
| Thinking | Budget 모드 | Level 모드 (none/low/medium/high) |
| Tools | GoogleSearch, CodeExecution | google_search, googleMaps, url_context, code_execution |
| 엔드포인트 | aiplatform.googleapis.com | cloudcode-pa.googleapis.com |

## 참조 소스

| 소스 | 경로 | 용도 |
|------|------|------|
| RisuExtractUtil | `/config/workspace/RisuExtractUtil/` | UI/UX, 테스트 파일 |
| new-arona-bot-mk-2 | `/config/workspace/new-arona-bot-mk-2/` | Vertex AI OAuth, GeminiProvider |
| vertex-ai-oauth | `/config/workspace/vertex-ai-oauth/` | OAuth 서버 라이브러리 |
| RisuAI | `/config/workspace/RisuAI/` | CBS 파서, HyPA v3 |
| risu-gca.js | `/config/workspace/RisuExtractUtil/risu-gca.js` | GCA 모델, tools, UI |

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

## 다음 세션 우선순위

1. **APK 빌드 + 실기기 테스트** — 수정 사항 검증
2. **오토파일럿 실행 로직** — 현재 UI만
3. **HyPA 자동 요약** — 현재 설정 UI만, 임베딩 연결 필요
4. **UI/UX 개선** — Figma 디자인 적용
5. **대용량 파일 최적화** — base64 인코딩 or 파일 경로 전달
