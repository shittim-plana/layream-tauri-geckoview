# session.json 직렬화 + tmp+rename 벤치마크

대상: `layream-app/src-tauri/src/persistence.rs` — `save_session`
(동일 코드 경로를 `workspace_save_session`도 사용)

측정한 것: `save_session`의 본문 그대로 —
`serde_json::to_string_pretty({ "messages": [...] })` → `fs::write(tmp)` → `fs::rename`.
실제 함수를 호출했고, 별도로 직렬화 단계만 따로 재서 분리값을 얻었다.

## 결과 (release build)

| 메시지 | 바이트 | min(ms) | **med(ms)** | p90(ms) | max(ms) | 직렬화 med(ms) | 판정 |
|---:|---:|---:|---:|---:|---:|---:|:--|
| 100 | 126,242 (~123 KB) | 0.359 | **0.418** | 0.494 | 0.535 | 0.134 | 유지 |
| 500 | 625,613 (~611 KB) | 0.997 | **1.053** | 1.130 | 1.290 | 0.707 | 유지 |
| 1,000 | 1,248,600 (~1.19 MB) | 2.013 | **2.297** | 2.607 | 3.505 | 1.473 | 유지 |
| 5,000 | 6,242,314 (~5.95 MB) | 12.433 | **12.905** | 13.486 | 17.194 | 8.036 | 유지 |

- 측정: 사이즈마다 5회 warmup 후 50회, 정렬해 백분위 추출.
- `직렬화 med` = `to_string_pretty`만의 중앙값. 나머지(`총 − 직렬화`)가 `write+rename` + `create_dir_all`.

## 판단

판정 기준 ( <100ms 유지 / 100–500ms 워치리스트 / >500ms 개선 ):

**네 사이즈 모두 <100ms → 전부 유지.** 워치리스트 진입조차 없다.

- 최악 케이스인 5,000 메시지(~6 MB)도 중앙값 12.9ms, max 17.2ms. 100ms 경계까지 ~6배 여유.
- 스케일은 메시지 수/바이트에 거의 선형 (100→5000, 50배 데이터에 med 0.42ms→12.9ms ≈ 31배).
- 비용 분해: 5,000에서 직렬화 8.0ms / fs(write+rename) ≈ 4.9ms. 메시지가 커질수록 직렬화 비중이 커진다 (CPU-bound).

### 경계 견고성

워치리스트 진입(100ms)까지의 여유가 디스크 속도 가정에 흔들리는지 확인:
- 직렬화(8ms@5000)는 CPU-bound — SSD/HDD 무관.
- fs 부분(~5ms@5000)이 디스크 의존. 측정 환경보다 10배 느린 디스크라도 fs ≈ 50ms, 직렬화 8ms 합 ≈ 58ms로 여전히 <100ms.
- 따라서 "유지" 결론은 디스크 종류 가정에 견고하다. (구조적 증명은 아님 — 50배 느린 디스크라면 깨짐. 다만 그 정도면 앱 전체가 이미 문제.)

## 측정 환경 (정직성 기록)

- build: **release** (`cargo test --release`). production 빌드 모드. debug serde_json은 수 배 느려 사용자 체감과 무관하므로 제외.
- rustc 1.94.1, serde_json 1.x (Cargo.toml `serde_json = "1"`).
- CPU: Intel Xeon @ 2.80GHz, 4 cores.
- 파일시스템: **ext4 on /dev/vda** (tmpfs/램디스크 아님 — write/rename이 실제 블록 디바이스 syscall을 친다).
- OS: Linux 6.18.5 컨테이너.

### 측정 과정에서 막혔던 지점 (재현 시 참고)

- 이 크레이트는 tauri(→wry→webkit/gtk)에 의존해서, 컨테이너 초기 상태에서는 `gdk-3.0`/`webkit2gtk` 부재로 컴파일 자체가 실패했다.
- `save_session` **본문은 tauri를 전혀 거치지 않지만**(serde_json + std::fs뿐), 같은 크레이트라 링크를 위해 시스템 라이브러리가 필요했다.
- 해결: `apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libxdo-dev libayatana-appindicator3-dev libssl-dev` 후 실제 함수로 측정. (대안: tauri 비의존 standalone bench로 동일 수치를 얻을 수 있으나, 실제 함수를 직접 호출하는 쪽을 택함.)

## 재현 방법

```
cd layream-app/src-tauri
cargo test --release --lib bench_session -- --nocapture --test-threads=1
```

벤치 코드: `persistence.rs` 하단 `#[cfg(test)] mod bench_session`.
메시지 형태는 프론트엔드(`messageStore.js` / `ChatView.svelte`)를 따랐다 —
`{ chatId, parentId, branchId, role, text, time, [pinned], [alternatives] }`.
content 크기는 매직값이 아니라 상수로 명시 (user 280자 / char(assistant) 1200자 / 4번째 char 턴마다 1200자 alternative 2개).
