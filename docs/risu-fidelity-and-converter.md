# RisuAI 포맷 충실도 + 정규식/스크립트 + acvus 변환기 — 설계·계획

> 상태: WIP (branch `risu-fidelity-wip`). 이 문서는 **계획·결정·근거를 박제**한 것.
> 무거운 구현(빌드/테스트가 큰 작업 — fancy/regress, 코퍼스, 실제 regex/CBS)은
> **별도의 고성능 code server에서** 진행한다. 이 문서가 그 인계서다.

---

## 0. 한눈에

세 갈래의 작업이 있고, 의존 순서가 있다:

```
Part 1 (Rust)   포맷 import/export 충실도   ← 데이터. 지금 진짜 손실. 최우선.
Part 2 (Rust)   정규식 전략                 ← 실행. RisuAI 호환의 핵심.
Part 3 (설계)   실행 위치 (Rust vs GeckoView)
Part 4 (후속)   CBS → acvus 변환기          ← Part 1의 무손실 import 위에 세움.
```

GIGO 원칙: **import가 손실이면 acvus로 무손실 변환이 불가능**하다. 그래서
Part 1(포맷 충실도)이 Part 4(변환기)보다 먼저다.

---

## 1. 리포 구조 & 단계 (phasing)

레포 역할이 고정됐다:

```
layream  (shittim-plana/layream, 이 repo, = 그 "별도 리포")
  · RisuAI 포맷 import/export
  · 정규식 / CBS / Lua 처리
  · CBS → acvus 변환기            ← 여기 들어감
acvus    (ArtBlnd/acvus, 그리고 shittim-plana/acvus 포크)
  · 엔진. 깨끗하게 유지. layream 기능을 안 넣음.
  · pomollu-tauri (네이티브 Tauri 호스트)는 포크 브랜치에 있음:
    shittim-plana/acvus  branch: tauri-apk-providers-etc
pomollu  (acvus repo 내부)
  · acvus 엔진을 소비. layream 기능을 재구현하지 않음.
```

단계:

```
지금 → acvus 베타 전:   layream ↔ RisuAI 한정 임시 workaround
                        (RisuAI 포맷을 충실히 import/export, 정규식/CBS 동작)
acvus 베타 후:          RisuAI → Layream (CBS→acvus) → Pomollu (acvus)
                        변환기 = layream의 별개 기능 (acvus 코어 밖)
```

근거: CBS→acvus를 acvus 코어에 올리면 그쪽 개발(ArtBlnd)에 잡음. 변환기는
**내(사용자) repo인 layream**에 두는 게 맞다. acvus 코어는 엔진만.

### 다른 code server 셋업 (cross-repo)

변환기는 acvus 코드를 **복사하지 않고 타깃 포맷으로 참조**한다. 그래서:

```
git clone shittim-plana/layream         # 작업 대상
git clone shittim-plana/acvus           # 변환 타깃 스키마 참조용
  └ branch tauri-apk-providers-etc 에 pomollu-tauri (참고: ReqwestFetch 엔진 훅,
    acvus-ext-llm mistral/vertex registry, FsBlobStore 등)
```

acvus 타깃 포맷(node spec / SessionConfig)은 `acvus-orchestration/src/spec/`,
프론트 타입은 `pomollu-frontend/src/lib/engine.ts` 참고.

---

## 2. Part 1 — RisuAI 포맷 I/O 충실도 (Rust, layream-core)

### 2.1 손실 메커니즘 두 종

```
M1. 타입드 구조체에 #[serde(flatten)] extra 누락
    → import 시 모델에 없는 필드 증발 → export가 복원 불가
M2. rmpv_to_json 의 타입 손실 (risup 경로의 JSON view)
```

### 2.2 rpack — 이상 없음

`rpack.rs`는 256-바이트 치환 테이블(역함수, 테스트 검증). 무손실. 손대지 않음.

### 2.3 catch-all 누락 구조체 (M1) — `layream-core/src/types.rs`

이미 안전(✓ extra 있음): `BotPreset`, `LoreBook`, `RisuModule`,
`CharBookEntry`, `RisuAiExtensions`, `CardExtensions`, `CharacterCardV2Data`.

**누락(손실점) — `#[serde(flatten, default)] extra: HashMap<String, Value>` 추가 대상:**

| 구조체 | 라인(대략) | 영향 |
|---|---|---|
| `CharacterCardV2Risu` | 838 | top-level {spec, spec_version, data}만 — V3 카드 top-level 필드 드롭 |
| `CharacterBook` | 731 | 카드 lorebook 컨테이너 — V3 book 필드 드롭 (entries는 CharBookEntry라 ✓) |
| `CustomScript` | 205 | **regex 스크립트** — RisuAI regex 객체 추가 필드 드롭 (흔함) |
| `TriggerScript` | 218 | 트리거 스크립트 추가 필드 드롭 |
| `NewGenData` / `DepthPrompt` | 777 / 786 | 카드 서브객체 |
| `LoreBookExtensions` / `LoreCache` | 194 / 199 | 마이너 |
| 파라미터 구조체들 | 343~509 | `PromptSettings`, `SeparateParameters`, `Ooba*`, `Nai*`, `Ain*` — 추가 필드 드롭 |

> 주의: `flatten` 추가는 단순 필드 보존만이 아니라 **2.4의 인코딩 버그도 같이 고친다**
> (flatten이 struct를 map으로 강제 직렬화하므로). 아래 참고.

### 2.4 risup export 인코딩 버그 (HIGH) — `layream-core/src/preset.rs:132` `encode_risup`

```rust
let inner_msgpack = rmp_serde::to_vec(preset)?;   // ← 문제
```

`rmp_serde::to_vec`(≠ `to_vec_named`)는 구조체를 **positional 배열**로 직렬화한다.
RisuAI는 msgpack을 JS object(named map)로 읽는다.

```
BotPreset:  flatten 있음 → serde가 map으로 강제 ✓ (named)
중첩 struct 중 flatten 없는 것 (CustomScript, TriggerScript, CharacterBook, 파라미터):
  → rmp_serde가 positional 배열로 인코딩 ✗
  → RisuAI는 {comment, in, out, ...} object 기대 → 못 읽음
```

즉 **regex/트리거/character_book이 든 프리셋을 export하면 그 부분이 배열로 나가
RisuAI가 못 읽는다.** 단순 프리셋만 우연히 동작(BotPreset의 flatten 덕). 이게
"살짝 손실"의 정체 — 실은 조용한 export 깨짐.

**수정**:
```
rmp_serde::to_vec  →  rmp_serde::to_vec_named   (모든 struct를 named map으로)
+ 2.3의 flatten extra 추가 (이중 안전: map 강제 + 미지 필드 보존)
```

> 검증: export→RisuAI 재import 대신, export한 msgpack을 디코드해
> `CustomScript`가 map인지 array인지 단언하는 round-trip 테스트로 박제.
> (이 버그는 rmp_serde 기본 동작에서 *도출*한 것 — 테스트로 *witness* 필요.)

### 2.5 risum export 부재 (HIGH) — `preset.rs`

`parse_risum_data`(import)만 있고 **`encode_risum`이 없다**. risum 내보내기 기능
자체가 미구현. import 시 assets를 base64로 펼치는데(`parse_risum_binary`),
되돌릴 writer가 없어 round-trip 불가.

**구현**: binary 컨테이너 writer
```
[magic=111][version=0][u32le main_len][rpack(json)]
  then per asset: [1][u32le len][rpack(data)]
  then [0] terminator
```
(import 코드 `parse_risum_binary`의 역. 상수 RISUM_MAGIC/VERSION/마커 재사용.)

### 2.6 rmpv_to_json — view-export 용 (LOW, 선택)

`rmpv_to_json`은 `msgpack_to_preset`의 import 브리지지만, **사람이 보는 JSON
표현**으로도 쓰려는 의도. JS-출신 데이터에선 손실 분기가 사실상 dead:

```
non-string 맵 키 드롭   → JS는 string 키만 → 발생 안 함
초과 정수 → Null        → rmpv Integer는 i64|u64뿐 → 분기 도달 불가
Ext 태그 손실           → JS→msgpack은 Ext 거의 안 씀 → 희귀
Binary → String/byte배열 → preset 자산은 보통 base64 *문자열* → 희귀
```

유일한 현실적 거친 케이스: **raw Binary 블롭이 int 배열로 나와 사람이 못 읽음.**

**수정(선택, view용)**: `Binary` → UTF8면 String, 아니면 **base64 문자열**
(int 배열 X). `Ext`는 `{ "__ext": tag, "data": base64 }`로 태그 보존.
dead 분기(non-string 키/초과 정수)는 손대지 않음.

### 2.7 round-trip 테스트 (Part 1 검증)

```
· preset:  실 risup → import → export(risup) → 재import = 동일 (필드·구조 보존)
· module:  risum → import → encode_risum → 재import = 동일 (assets 포함)
· card:    charx/png → import → (export) → 재import = 동일
· 인코딩:  export한 msgpack 디코드 → CustomScript가 map (배열 아님) 단언
```

---

## 3. Part 2 — 정규식 전략

### 3.1 문제

`layream-core/src/regex.rs`의 `apply_regex`:
```rust
if let Ok(re) = Regex::new(&script.pattern) {        // 표준 regex 크레이트
    result = re.replace_all(&result, script.out.as_str()).into_owned();
}   // Err면 조용히 스킵 — 에러도 없음
```

문제들:
```
· script.pattern = RisuAI "in" 필드 = 보통 /pattern/flags 형식
  → 그대로 Regex::new 하면 Err → 조용히 스킵 (기본 regex도 깨질 수 있음)
· 표준 regex 크레이트는 JS의 lookaround/backreference 미지원 → Err → 조용히 스킵
· /flags (i,g,m,s,u) 미적용
· JS 치환 문법 ($& $<name> $') ≠ Rust 치환 ($0 ${name})
· script type (editinput/output/display/process) 디스패치 없음 (전부 일괄 적용)
```

→ lookaround/backref 쓰는 RisuAI 스크립트가 **에러 없이 사라진다**. 이게 regex
포팅 이슈의 정체(조용한 정합성 손실). RisuAI는 roleplay regex라 lookaround 흔함.

> 소스 오브 트루스: **RisuAI 원본**(shittim-plana/RisuAI)의 regex 처리부.
> layream의 (그리고 RisuAI fork의) **하이라이터는 같은 버그가 있어 신뢰 금지** —
> 파싱/실행부만 참조.
> 참고: nevaeh5379 CBS_LLM_REFERENCE, tresbien-rai/risuai-cbs-editor.

### 3.2 엔진 후보와 "뭘 잃나" (ArtBlnd 원칙)

```
표준 regex 크레이트
  + 선형시간 보장(ReDoS 면역), 최상위 성능
  − lookaround/backreference 불가 (non-regular → 정리상 불가능)
fancy-regex
  + lookaround/backreference 가능, backtrack_limit 가드 내장, replace API 내장
  − ReDoS 면역 상실, 느림, PCRE-ish라 JS와 미세 비호환(char class/flag)
regress  (ECMAScript regex 엔진, Boa가 씀)
  + JS 의미론 가장 정확 ("set 다름" 문제 해소)
  − ReDoS 면역 상실, 느림, niche, **replace 없음(직접 구현)**,
    backtrack 가드 유무 불확실(검증 필요), v/d flag 등 잔여 비호환
GeckoView (webview의 JS RegExp)
  + 100% JS 충실 (SpiderMonkey)
  − RisuAI의 성능 문제 상속 (아래 3.4)
```

핵심 정리(non-regular): `(a+)\1` = {ww}는 정규언어가 아니다(pumping lemma).
**backreference를 순수 표준 regex 크레이트로 변환하는 건 불가능** — 추가 장치로도
안 됨. 살리려면 backtracking 엔진(fancy/regress) 또는 JS 엔진 필요.

### 3.3 결정: 코퍼스 기반 + 성능 우선 + 하이브리드

**성능이 목적**이므로(아래 3.4) 가장 빠른 **표준 regex가 기본**이어야 한다.
regress/fancy는 backtracking이라 느리다 → 성능과 충돌. 그래서:

```
RisuAI 실사용 regex 코퍼스 수집 (3 소스: preset/module/card)
  → /flags 벗긴 뒤 표준 regex::Regex::new 에 넣어 통과율/실패원인 분류
     (컴파일만, 매칭 안 함 → ReDoS 무관, 안전)
  → 결정:
     거의 다 통과         → 표준 regex 채택 (빠르고 안전, 무손실). 끝.
     lookaround/backref 꼬리 유의미 → 하이브리드:
        표준 먼저 시도 → Err면 그때만 regress/fancy로 폴백
        → ReDoS 위험을 "정말 backtracking 필요한 꼬리"로만 한정
```

엔진 동률 시 JS 정확도는 regress > fancy. 단 regress는 replace 직접 구현 +
backtrack 가드 검증 필요. 폴백 엔진 최종 선택은 코퍼스 결과 + regress API 확인 후.

부수 구현(엔진 무관, 필수):
```
· /pattern/flags 파싱 → (?i)(?m)(?s) 인라인 변환, g → replace_all vs replace
· JS 치환 문법 번역: $& → ${0}, $<name> → ${name}, $$ → $   ($' `$\`` 는 코드 필요)
· script type 디스패치 (호출부가 input/output/display/process로 사전 필터)
· 컴파일 실패 silent skip 제거 → 로그/표면화 (어느 스크립트가 왜 실패했는지)
```

코퍼스: git에 안 올림(라이선스+크기). 로컬 fixture(gitignore). 작은 합성 fixture만 커밋.
표본 후보: CBS·regex 빡센 프리셋(보유), 봇카드(수집 필요), RisuAI default 스크립트.

### 3.4 왜 Rust인가 — 성능 (GeckoView를 안 쓰는 이유)

RisuAI 자체가 느리다. GeckoView(webview JS)에서 실행하면 **그 성능 문제를 상속** →
Rust 네이티브로 만드는 의미가 없어진다. 그래서 **실행은 Rust**.

진짜 성능 이득 위치:
```
CBS 평가       Rust >> JS  (CPU-bound, 큰 이득)
lorebook 스캔  Rust >> JS  (O(entries×history), RisuAI의 진짜 병목)
Lua           네이티브 mlua/LuaJIT >> wasmoon(WASM)  (큰 이득)
regex         Rust > JS 지만 작은 차 (JS RegExp도 JIT)
```

일관성: 파이프라인(CBS+regex+lorebook)이 Rust면 regex만 webview로 못 뗀다
(호출마다 JS 경계 왕복). 전부-Rust 아니면 전부-webview. 성능 → 전부-Rust.

성능이 표준 regex를 선호하는 이유와도 합치: 표준 regex가 가장 빠름 → 코퍼스가
허락하면 표준 + 꼬리 폴백 = 빠름+충실 둘 다.

---

## 4. Part 3 — 정규식의 3 소스 & Lua

정규식은 세 곳에서 온다(코퍼스/디스패치가 셋 다 다뤄야 함):
```
프롬프트 프리셋: BotPreset 의 regex/customscript
모듈:          RisuModule.regex (Vec<CustomScript>)
캐릭터 카드:    CardExtensions.risuai.custom_scripts (Vec<CustomScript>)
```

Lua: RisuAI 스크립팅은 Lua (triggerscript / cjs / virtualscript / low-level).
regex와 별개 실행 환경. 성능 위해 **네이티브 Lua(mlua/LuaJIT)** 권장
(webview wasmoon은 RisuAI 성능 상속). 단 이건 별도 작업 — 본 문서 범위 밖,
후속으로 분리.

---

## 4.5 layream 저장소 개선 — content-addressed store 재사용

pomollu-tauri(acvus 포크 branch `tauri-apk-providers-etc`)에 만든
`FsBlobStore`(`pomollu-core/src/store.rs`)는 **acvus엔 없던 새 능력**이고,
**layream도 같은 문제**가 있다 — 현재 layream의 세션/워크스페이스/히스토리는
mutable JSON(`persistence.rs`, tmp+rename, last-write-wins).

그 store를 layream-core로 포팅하면 동일 이득:
```
content-addressed blobs (SHA-256, G-Set) + append-only journal (parent DAG)
  + CAS pointer + 멀티윈도(cas_lock, commit, branch-on-conflict)
→ 채팅 히스토리·undo·분기(스와이프)·멀티윈도 안전을 mutable JSON 없이
```

이식 메모:
```
· store.rs는 순수 Rust, 의존성 적음(sha2/hex + 자체 error). layream-core에
  새 모듈로 복사 후 error 타입만 LayreamError로 어댑트.
· layream의 session/branch 모델(messageStore의 branchId/headId/forkPoint)이
  이미 DAG라 journal의 parent 링크와 자연스럽게 맞음.
· RisuAI 스와이프/대안응답 = journal의 sibling children = 무손실 분기.
· 단계: (1) store 포팅 + 테스트, (2) 세션 저장을 store 위로, (3) 워크스페이스.
```

우선순위: Part 1(포맷 충실도)·Part 2(regex) 뒤. layream "개선" 묶음의 일부.

---

## 5. Part 4 — CBS → acvus 변환기 (후속, 베타 후)

### 5.1 핵심: 변환기는 regex를 *실행*하지 않고 *번역*만 한다

그래서 Part 2의 ReDoS/엔진 딜레마가 **변환 시점엔 적용 안 됨**. 변환기엔
"실행기"가 아니라 "스캐너"만 필요(매칭 안 하니 ReDoS 무관).

```
RisuAI regex 스크립트 → acvus regex-transform spec (pattern/out/flags/type 보존)
  + JS-전용 기능 정적 감지(lookaround/backref/flags/$& 류) → lossy 진단으로 표면화
  + 번역 가능한 것 transpile (/flags → 인라인, $& → ${0})
→ 실행 엔진 선택은 acvus 런타임의 몫 (변환기와 분리)
```

즉 변환기는 "조용한 런타임 드롭"을 "정적 컴파일타임 경고"로 바꾼다(개선).

### 5.2 CBS → acvus 매핑 (3 구역)

CBS 레퍼런스는 ~150개 함수(별도: nevaeh5379 CBS_LLM_REFERENCE 참조). acvus
(typed + effect-tracked) 관점에서 3구역:

```
구역 A — CLEAN 매핑
  context reads (char/user/persona/description/scenario…) → acvus 타입드 @context
  변수 (getvar/setvar/addvar/tempvar) → acvus context + persistency
    ★ persistent chat var = 턴마다 변하는 상태 = acvus sequence/patch persistency
      = pomollu-tauri의 FsBlobStore 저널 그 자체 (content-addressed + journal + CAS)
  순수 연산 (equal/replace/makearray/dictelement/calc/?) → acvus expr/script 노드
  #when / #each / #func → acvus 조건부 / iterator / is_function 노드

구역 B — LOSSY (재구조화)
  random/randint/dice/roll/time/date → acvus는 effect-tracked
    CBS는 인라인으로 효과를 숨김, acvus는 Effect::Io extern으로 명시 (§1.3 density)
    → 1:1 아님. 변환기가 effect로 끌어올림.
  pick/rollp/hash (chat-id+char-id+msg-idx 해시 결정적) → context seed의 결정적 함수

구역 C — 범위 밖 (DROP 또는 프론트 메타데이터)
  display-only (asset/button/image/bg/bgm/inlay/tex/ruby…) → 모델에 안 감
    (acvus는 LLM 오케스트레이션이지 UI 렌더링 아님; 단 inlayeddata는 모델로 감)
  post-process (bkspc/erase) → 출력 transform, edge
```

CBS 함정(파서가 정확히 다뤄야 — 하이라이터 버그의 원인):
```
· #when 연산자 우→좌 우선순위 ({{#when::keep::not::A}})
· {{? }}는 :: 아니라 공백 구분 ({{? 2+3*4}})
· description/personality는 재귀 파싱 (필드 안에 CBS 또 있음)
· #escape/#puredisplay 블록은 내부 미파싱
· 닫힌 태그셋 — 모르는 {{...}}는 리터럴 보존
```

---

## 6. 결정 로그 & 미해결(residual)

결정:
```
D1. acvus 코어 안 건드림. 변환기는 layream(내 repo)에.
D2. 실행은 Rust (성능; GeckoView는 RisuAI 느림 상속이라 탈락).
D3. regex 기본 = 표준 크레이트(가장 빠름), 코퍼스가 요구하는 꼬리만 backtracking 폴백.
D4. 변환기는 번역만(실행 X) → ReDoS/엔진 딜레마 변환 시점엔 무관.
D5. 포맷 충실도(Part 1)가 변환기(Part 4)보다 먼저 (GIGO).
D6. GCA 자격증명(GOCSPX 등)은 비밀 아님 — 공개 인증용. acvus 포크는 비공식이라
    GCA는 master 아닌 피처 브랜치에만.
```

미해결:
```
R1. 코퍼스 통과율 — 표본 수집 후 측정해야 표준 vs 폴백 비율 확정.
R2. regress API — backtrack 가드 유무, 매칭/캡처 시그니처, replace 부재 확인.
R3. 봇카드 표본 수집.
R4. Lua 실행(네이티브 mlua vs wasmoon) — 별도 작업, 범위 밖.
R5. charx/png export 경로 — 현재 read_*만 있음. export가 어디서/어떻게 일어나는지
    확인 후 자산·PNG 청크·원본 이미지 보존 검증.
R6. CBS→acvus의 acvus 타깃 표현 — acvus-orchestration spec 확정(베타) 후 1:1 매핑.
```

---

## 7. 작업 분담 (서버)

```
이 서버 (가벼움)        : 본 문서(설계·결정 박제). git 메타작업.
다른 code server (무거움): 빌드·테스트 큰 것 —
  · Part 1 수정(types.rs flatten, to_vec_named, encode_risum) + round-trip 테스트
  · Part 2 regex (코퍼스 측정 → 표준/폴백 → /flags·치환·type·silent-skip)
  · Part 4 변환기 (베타 후)
  툴체인 주의: 이 박스는 C 컴파일러 없어 zig cc 우회 + ring 빌드 느림.
  고성능 서버에선 정상 툴체인 가정.
```

---

## 8. 참고 (witness 위치)

```
layream-core/src/preset.rs       read_preset, encode_risup(:132), parse_risum, rmpv_to_json(:75)
layream-core/src/types.rs        구조체 정의 (flatten 누락 표 §2.3)
layream-core/src/regex.rs        apply_regex (silent skip §3.1)
layream-core/src/rpack.rs        무손실 치환 (손대지 않음)
RisuAI (shittim-plana/RisuAI)    regex/CBS 소스 오브 트루스 (하이라이터 제외)
acvus 포크 branch tauri-apk-providers-etc  pomollu-tauri (변환 타깃 + 엔진 훅 참고)
```
