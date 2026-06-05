# HyPA 경로 확인

조사 범위: `layream-core/src/hypa.rs`, `layream-app/src-tauri/src/commands_hypa.rs`,
프론트엔드 RAG 호출 경로. 모든 주장은 `file:line` 근거를 단다.

---

## 0. 한눈에

| 질문 | 결론 |
|---|---|
| `select_memories` ↔ `hypa_search` 중복? | **아니오 — 별개.** 시그니처·알고리즘·출력이 다르다. |
| Voyage 임베딩 사용 경로? | **있음.** 단, 쿼리 측에만. 요약 측은 Vertex 고정 → 비대칭. |
| 조치 | 별개이므로 **문서화**. 통합 코드 없음. 다만 별도의 진짜 중복(타입 2중 정의 + 데드코드)을 §4에 기록. |

---

## 1. 정의 — 관측된 것

### 1.1 두 구현의 입출력 (Definition)

**`select_memories`** — `layream-core/src/hypa.rs:75`
```
fn select_memories(
    data, embedded: &[EmbeddedSummary],
    query_embeddings: &[Vec<f64>],   // 다중 쿼리
    query_weights: &[f64],
    token_budget, estimate_tokens, settings: &HypaSettings,
) -> SelectionResult { selected, important, recent, similar, random }
```
- 4단계 선택: ① important 전량 → ② recent 역순 → ③ similar(다중 쿼리 RRF, `hypa.rs:142`) → ④ random.
- 토큰 예산(`token_budget`)을 단계별로 배분·재배분(`recent_realloc`, `similar_realloc`).
- 점수: RRF `1.0 / (60.0 + rank)` × weight (`hypa.rs:142-143`).

**`hypa_search`** — `layream-app/src-tauri/src/commands_hypa.rs:479`
```
async fn hypa_search(
    query_embedding: Vec<f64>,        // 단일 쿼리
    top_k: usize, ...
) -> Result<Value, String>           // [{ index, score, summary }]
```
- 단일 단계: `invalidated` 제외 → 차원 일치 검사 → `cos + pin_boost` → 정렬 → `truncate(top_k)` (`commands_hypa.rs:497-517`).
- 점수: 코사인 + 핀 부스트(`commands_hypa.rs:509-510`). 토큰 예산 개념 없음.

### 1.2 임베딩 생성 경로 (Definition)

| 함수 | 위치 | 공급자 | 호출 시점 |
|---|---|---|---|
| `embed_text_vertex` | `commands_hypa.rs:341` | **Vertex 고정** | 요약 생성 시 (`hypa_summarize`, `commands_hypa.rs:436`) |
| `embed_vertex` (cmd) | `commands.rs:1107` | Vertex | RAG 쿼리 (provider="vertex") |
| `embed_voyage` (cmd) | `commands.rs:1124` | **Voyage** | RAG 쿼리 (provider="voyage") |
| `voyage::embed` | `layream-core/src/voyage.rs:50` | Voyage HTTP | 위 `embed_voyage`가 호출 |

프론트 분기: `assemblePrompt.js:35` `const provider = h.embeddingProvider || "vertex"` →
`embed_vertex`(`:39`) 또는 `embed_voyage`(`:47`).

### 1.3 배선 (Definition)

- `hypa_search`는 핸들러 등록됨: `lib.rs:115`. 프론트 호출: `HypaView.svelte:346`
  (`getRagContext` 래퍼 `:338` → ChatView RAG에서 사용).
- `select_memories`는 **레포 전체에서 호출자 0개** (테스트 `hypa.rs:255` 제외).
  `grep -rn select_memories` 결과: 정의 1, 테스트 1. 그 외 없음.
- `layream-core/src/lib.rs`는 `pub mod hypa;`만 선언. `EmbeddedSummary`, `HypaSettings`,
  `build_memory_block`, `clean_orphaned`, 코어 `Summary`/`HypaData` 모두
  파일 밖 참조 0개 (`grep` 확인).

---

## 2. 도출 — 정의에서 따라 나오는 것

### 2.1 중복이 아니다 (Derivation)

전제(§1.1):
- `select_memories.input ≠ hypa_search.input` (다중 쿼리+예산 vs 단일 쿼리+top_k).
- `select_memories.output ≠ hypa_search.output` (분류된 인덱스 vs 점수 객체 배열).
- 점수 함수가 다름 (RRF vs 코사인+핀부스트).

→ 동일 규칙을 두 번 구현한 것이 아니라, **서로 다른 규칙**이다.
CLAUDE.md §4.1 "similar: same_rule → unify; ¬same_rule → separate" 기준상 **separate**.

개념적 포함 관계는 있다: `select_memories`의 ③ similar 단계가 다중 쿼리 RRF로
`hypa_search`의 코사인 정렬을 일반화한 상위집합에 가깝다. 그러나 핀 부스트·
`invalidated` 필터는 `hypa_search`에만 있고, 토큰 예산·important/recent/random은
`select_memories`에만 있다. **상호 대체 불가.**

분류: non-trivial / 현실 부합 (grep 근거).

### 2.2 Voyage 경로는 존재하나 비대칭 (Derivation)

전제(§1.2):
- 요약 임베딩 = Vertex 고정 (`embed_text_vertex`는 Vertex만 호출, `commands_hypa.rs:366-377`).
- 쿼리 임베딩 = Vertex 또는 Voyage (`assemblePrompt.js:35`).

→ 사용자가 `embeddingProvider="voyage"`를 고르면:
쿼리는 Voyage 공간, 저장된 요약은 Vertex 공간.
`hypa_search`의 코사인은 **서로 다른 임베딩 공간** 간 계산 → 무의미.

세부 분기:
- 차원이 다르면 (`commands_hypa.rs:504` `emb.len() != query_embedding.len()`) 전부 skip
  → `hypa_search`가 **조용히 빈 결과** 반환. RAG가 작동하지 않음.
- 차원이 우연히 같으면 → 의미 없는 유사도로 잘못된 요약 주입.

CLAUDE.md §1.3 "mismatch at entry_point: if ¬corrected → propagates downstream" 에 해당하는
**잠복 버그**. 다만 §5.1 기준 "조용한 무시"는 아님 — 차원 불일치 skip은 의도된 방어
(`commands_hypa.rs:505-506` 주석 §3-A). 문제는 한 단계 위, 공급자 선택 불일치다.

분류: non-trivial / 현실 부합 / 미해결 위험.

### 2.3 진짜 중복은 따로 있다 (Derivation)

전제(§1.3):
- `Summary`/`HypaData`가 두 곳에 정의됨: `hypa.rs:8,15` 와 `commands_hypa.rs:78,106`.
- 코어 `hypa` 모듈 전체(타입+함수)가 프로덕션에서 참조 0.

→ CLAUDE.md §4.1 "exists exactly_one site s where defined(f, s)" **위반**:
같은 개념(`Summary`, `HypaData`)이 두 site에 정의. 코어 쪽은 데드.
→ §3.2 기준 코어 `hypa` 모듈은 production(solution space 축소)에 기여하지 않음 = 데드코드.

분류: non-trivial / 현실 부합.

---

## 3. 분류 — 조치

### 3.1 통합 안 함 (named 비교: select_memories ↔ hypa_search)

`select_memories`를 프로덕션에 배선하려면 프론트 RAG 계약(토큰 예산, 다중 쿼리,
반환 형태)을 바꿔야 한다. 이는 분석 과업의 범위 밖(§4.2 scope) + 동작 변경.
두 함수가 별개이므로 **통합 코드는 작성하지 않는다** (과업 분기: 별개 → 문서화).

### 3.2 권고 (실행 전 합의 필요)

세 가지, 비용·범위 순:

1. **임베딩 비대칭 수정** (정합성, 권장 1순위)
   - 요약도 `embeddingProvider`를 따르게 하거나 (`embed_text_vertex` → 공급자 분기),
   - 또는 요약에 사용한 공급자를 메타로 기록하고 `hypa_search`/`embedQueryForRag`에서
     공간 일치를 강제. 현재는 한쪽만 Voyage가 되는 순간 RAG가 침묵.

2. **타입 2중 정의 정리** (§4.1)
   - 코어 `hypa::{Summary, HypaData}`는 데드. 프로덕션 타입은 `commands_hypa` 쪽.
   - 하나로 수렴(코어를 정식 타입으로 올리고 app이 재사용) 또는 코어 데드 제거.

3. **데드 모듈 제거** (§4.3 complete_removal)
   - `select_memories`, `EmbeddedSummary`, `HypaSettings`, `build_memory_block`,
     `clean_orphaned`, 코어 `Summary`/`HypaData` 및 해당 테스트.
   - 단, 삭제는 비가역 + 구조 변경. **사용자 확인 후 진행** (CLAUDE.md Agreement).
   - 보존 근거가 있다면(향후 토큰 예산 기반 선택 도입 예정 등) 유지 + 의도 주석화가 대안.

이 문서는 ①~③을 **기록만** 한다. 실행은 별도 합의 사항.

---

## 4. 근거 색인

| 사실 | 근거 |
|---|---|
| `select_memories` 정의 | `layream-core/src/hypa.rs:75` |
| RRF 점수 | `layream-core/src/hypa.rs:142-143` |
| `select_memories` 호출자 0 | `grep -rn select_memories` → 정의/테스트만 |
| `hypa_search` 정의 | `layream-app/src-tauri/src/commands_hypa.rs:479` |
| 코사인+핀부스트 | `commands_hypa.rs:509-510` |
| 차원 불일치 skip | `commands_hypa.rs:504-507` |
| 핸들러 등록 | `layream-app/src-tauri/src/lib.rs:115` |
| 프론트 호출 | `HypaView.svelte:346`, 래퍼 `:338` |
| 요약 임베딩 Vertex 고정 | `commands_hypa.rs:341,366-377,436` |
| 쿼리 임베딩 공급자 분기 | `assemblePrompt.js:35,39,47` |
| Voyage HTTP | `layream-core/src/voyage.rs:50` |
| 타입 2중 정의 | `hypa.rs:8,15` vs `commands_hypa.rs:78,106` |
| 코어 모듈 export | `layream-core/src/lib.rs:6` (`pub mod hypa;`) |
