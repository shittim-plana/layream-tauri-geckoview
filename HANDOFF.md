# Handoff — 현재 상태 요약 (2026-05-03)

**CLAUDE.md를 반드시 전부 읽고 모든 지침을 따를 것.**
**참조 소스 없이 추측 금지 — 모델명, API 구조, UI 패턴 전부 원본 코드에서 확인.**

## 최신 커밋
- `6be7af1` (2026-05-03)
- 릴리스: v0.2.1-alpha (prerelease, signed APK)

## 이번 세션 완료 사항

### 코드 리뷰 + 버그 수정 (20건)
- invoke 파라미터 camelCase→snake_case (28개 전수 대조 PASS)
- deep-link 권한 추가, GCA tools 직렬화 수정
- AES-GCM 랜덤 nonce, 토큰 리프레시 추가
- CBSEditor 중복 invoke 제거, 채팅 자동 스크롤
- FileImport 타임아웃 수정, 디버그 메시지 정리
- HyPA 설정 영속화, SettingsView load-merge-save
- preset.customscript→regex, assistantPrefill 경로 수정

### 추가 수정
- GCA OAuth: loopback 서버 방식 (localhost redirect, client_secret)
- OAuth prompt: select_account consent (계정 선택 강제)
- 채팅 입력: 컴팩트 textarea (1줄→자동 확장→max 120px)
- 파일 임포트: dialog 플러그인 제거, HTML input 직접 사용 (user gesture 보존)
- keystore: 리포에서 제거, .gitignore 추가
- APK 서명: apksigner로 서명된 APK 릴리스

## 미해결 — 다음 세션 우선순위

### Critical (기능 불가)

#### 1. OAuth 계정 선택 안 뜸
`prompt=select_account consent` 추가했으나 여전히 계정 선택 화면 안 뜸.
가능한 원인:
- GCP OAuth 동의 화면이 "테스트" 모드 → 테스트 사용자에 본인 이메일 미등록
- 또는 "프로덕션" 게시 필요
- Vertex AI: 커스텀 URI 스킴 활성화 완료 (GCP Android 클라이언트)
- GCA: loopback 서버 방식으로 구현 완료, 실기기 미검증

**다음 조치:**
1. GCP Console → OAuth 동의 화면 → 테스트 사용자에 본인 Gmail 추가 (또는 프로덕션 게시)
2. 재테스트

#### 2. 프리셋 내보내기 안 됨
Export .risup / Export .json 버튼이 동작하지 않음.
`export_preset` invoke는 구현되어 있으나, Android WebView에서 Blob URL → `<a>.click()` 다운로드가 동작하지 않을 수 있음.
**다음 조치:** Android에서 파일 저장은 Tauri fs 플러그인 `writeFile` 또는 `tauri-plugin-dialog`의 save dialog 사용 필요

### Medium (UX 문제)

#### 3. 채팅 입력이 아래로 스크롤해야 보임
채팅 영역이 길어지면 입력칸이 화면 밖으로. 입력칸을 화면 하단에 고정(sticky) 필요.
**다음 조치:** CSS `position: sticky; bottom: 0;` 또는 flex layout 재구성

#### 4. 탭 전환 시 채팅 기록 소실
Svelte 컴포넌트가 탭 전환 시 파괴→재생성됨. 로컬 state가 초기화.
**다음 조치:** 
- 채팅 기록을 App.svelte 레벨로 올리거나 (prop으로 전달)
- 또는 Tauri 백엔드에 세션 저장 (cmd_save/load_session)
- 또는 `{#if}` 대신 CSS display:none으로 탭 전환 (컴포넌트 파괴 방지)

#### 5. 채팅 세션 저장/불러오기 기능 없음
앱 재시작 시 채팅 기록 손실. 세션 영속화 필요.
**다음 조치:** session.json 저장/불러오기 커맨드 구현

#### 6. 파라미터 -1000 표시
RisuAI에서 "미사용" 의미의 -1000 값이 UI에 그대로 노출.
**다음 조치:** PresetView Parameters 탭에서 -1000 값을 빈 칸 또는 "default"로 표시

#### 7. Android 파일 선택기 타입 혼합
HTML input의 `accept` 속성을 Android가 무시. 프리셋 탭에서 .charx 선택 가능.
확장자 검증은 코드에 있으므로 잘못된 파일은 거부되지만, 사용자 경험이 혼란.
**다음 조치:** 파일 선택 후 확장자 에러 메시지를 더 명확하게 표시

#### 8. Vertex AI 모델 목록 부족
기본 목록에 gemini-2.5-pro, gemini-2.5-flash만 있음. Fetch 버튼은 OAuth 연결 후 동작.
**다음 조치:** 기본 모델 목록 확장 (vertex_api의 실제 가용 모델 참조)

### Low (기능 개선)

#### 9. 멀티 프리셋 지원
현재 1개만 로드 가능. 여러 프리셋 저장/비교 기능.

#### 10. 대용량 파일 최적화
Array.from(bytes) → 파일 경로 전달 방식으로 변경 (IPC 부하 제거)

#### 11. 오토파일럿 실행 로직
현재 UI만 존재, 실행 로직 미구현.

#### 12. HyPA 자동 요약
설정만 있고 실행 로직 미구현. 임베딩 연결 필요.

#### 13. UI/UX 개선 — Figma 디자인 적용

## OAuth 설정 상태

### Vertex AI OAuth
- GCP 프로젝트: `elite-totem-489721-s3` (본인 소유)
- Client ID: `317210024447-v4g6e0e1q5933vogajp0651vhkrgal06.apps.googleusercontent.com` (installed 타입)
- Android 클라이언트: 생성 완료 (패키지 `com.shittimplana.layream`, SHA1 `66:66:9B:92:BB:2E:5A:43:19:BB:64:17:14:26:74:A6:6A:08:B7:11`)
- 커스텀 URI 스킴: 활성화 완료
- Redirect URI: `com.shittimplana.layream://oauth/callback`
- 방식: PKCE (client_secret 없음)
- 동의 화면: 앱 이름 확인 필요 (현재 "Kivo"로 되어 있을 수 있음)
- **중요:** OAuth 동의 화면 → 테스트 사용자에 본인 Gmail 추가 필요

### GCA
- Client ID: `681255809395-...` (Google 공식)
- Client Secret: `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` (공개 값)
- 방식: loopback 서버 (localhost redirect, client_secret, PKCE 없음)
- Scope: cloud-platform + userinfo.email + userinfo.profile
- RisuAI(risu-gca.js) 레퍼런스 기반

### Vertex AI OAuth vs GCA — 완전 별개
| | Vertex AI OAuth | GCA |
|--|----------------|-----|
| Client ID | 317210024447-... | 681255809395-... |
| 인증 방식 | PKCE | client_secret |
| Redirect | custom scheme (deep-link) | loopback (localhost) |
| Project ID | 필수 (수동 입력) | 불필요 (loadCodeAssist 자동) |
| Thinking | Budget 모드 | Level 모드 |
| Tools | GoogleSearch, CodeExecution | google_search, googleMaps, url_context, code_execution |

## 참조 소스

| 소스 | 경로 | 용도 |
|------|------|------|
| RisuExtractUtil | `/config/workspace/RisuExtractUtil/` | UI/UX, 테스트 파일 |
| new-arona-bot-mk-2 | `/config/workspace/new-arona-bot-mk-2/` | Vertex AI OAuth, GeminiProvider |
| vertex-ai-oauth | `/config/workspace/vertex-ai-oauth/` | OAuth 서버 라이브러리 |
| RisuAI | `/config/workspace/RisuAI/` | CBS 파서, HyPA v3 |
| risu-gca.js | `/config/workspace/RisuExtractUtil/risu-gca.js` | GCA 모델, tools, UI, OAuth 레퍼런스 |

## Android 빌드 (클린 빌드 + 서명 필수)

```bash
sudo apt-get install -y -qq gcc libxml2 xz-utils unzip
source scripts/env.sh
cd layream-app
rm -rf ../target/aarch64-linux-android/release/deps/liblayream_app*
rm -rf ../target/aarch64-linux-android/release/liblayream_app*
rm -rf src-tauri/gen/android/app/build
npm run tauri android build -- --apk --target aarch64

# 서명 (keystore는 리포 외부에 보관)
$ANDROID_HOME/../sdk/build-tools/35.0.0/apksigner sign \
  --ks ../layream.keystore --ks-key-alias layream \
  --ks-pass pass:layream123 --key-pass pass:layream123 \
  --out /tmp/layream-signed.apk \
  src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk
```

## 사용자 피드백 메모
- Vertex AI OAuth와 GCA는 반드시 완전 분리
- OAuth 구현에서 사용자가 본인 Google 계정으로 로그인 → 본인 GCP 프로젝트 비용 부담 구조
- Client ID는 앱 식별자 (사용자별 아님), Project ID가 사용자별
- GCA UI는 risu-gca.js 플러그인 레퍼런스
- 공개 리포이므로 민감 정보 주의 (keystore 등)
- APK는 반드시 서명 후 배포
