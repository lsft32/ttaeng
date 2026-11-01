# 땡해줘 — PRD

| 항목 | 내용 |
|---|---|
| 문서 버전 | 1.1 (2026-11-02 업데이트 반영) |
| 작성일 | 2026-11-01 |
| 근거 | `01-requirements.md` v1.2 |

---

## 1. 제품 정의
로그인 직후 화면 오른쪽 아래에 작은 패널을 띄워 "얼음! 7/12 → 땡! (38초, 등급 B)"처럼 진행 상황을 보여주고, 끝나면 사라진다. 상세 화면에서 타임라인·이력·추천을 본다.

## 2. 기술 스택
| 영역 | 선택 | 이유 |
|---|---|---|
| 앱 셸 | Tauri 2.12 | zzap-easy에서 검증한 스택, Windows·Mac 공통 |
| 수집 | Rust `sysinfo` 0.40 (`system` 기능만) | 프로세스별 CPU(코어 1개 기준 %)·디스크 바이트·부모 PID·시작 시각을 두 OS에서 같은 API로 조회 |
| Windows 레지스트리 | `winreg` 0.56 | Run 키·StartupApproved 읽기, Run 키 자동 시작 등록 |
| Mac plist | `plist` 1 | LaunchAgents 읽기 |
| 중복 실행 방지 | `tauri-plugin-single-instance` 2 | 자동 시작과 수동 실행이 겹칠 때 |
| 프런트 | HTML/CSS/JS (빌드 도구 없음) | 화면 2개뿐이라 번들러 불필요. `withGlobalTauri` 사용 |

## 3. 구조

```
ttaeng/
├─ docs/                     문서·진행기록
├─ package.json              Tauri CLI만 사용
├─ ui/                       프런트 (frontendDist)
│  ├─ overlay.html/.js       실시간 패널
│  ├─ details.html/.js       타임라인·이력·추천
│  └─ common.css/.js         공통 스타일·포맷 함수
└─ src-tauri/src/
   ├─ main.rs                진입점
   ├─ lib.rs                 Tauri 설정, 트레이, 명령, 세션 시작
   ├─ model.rs               공용 타입 (항목, 화면 상태, 부팅 기록)
   ├─ judge.rs               판정 엔진 (순수 로직, 단위 테스트 대상)
   ├─ collector.rs           sysinfo 조회 → 프로세스 표
   ├─ session.rs             1초 루프, 이벤트 발행, 기록 저장
   ├─ network.rs             연결 확인
   ├─ advice.rs              등급·추천 규칙 (순수 로직, 단위 테스트 대상)
   ├─ update.rs              업데이트 확인·설치 (측정 중 차단, 6시간 간격)
   ├─ history.rs             이력·설정·캐시 파일
   ├─ autostart.rs           자동 시작 등록 (OS별)
   ├─ platform.rs            기준 프로세스명·시스템 경로·백그라운드 목록 (OS별 상수)
   └─ startup/
      ├─ mod.rs              공통 타입, 자동 감지 규칙
      ├─ windows.rs          Run 키, 시작 폴더, .lnk 해석, 로그온 작업 캐시
      └─ macos.rs            LaunchAgents
```

## 4. 데이터 흐름

```
[session 스레드, 1초마다]
 collector.sample() ──► ProcTable{pid → ppid, exe, start, cpu, disk}
 network 스레드 ──────► AtomicBool 연결됨/시각
        │
        ▼
 judge.tick(t, ProcTable, net)  ── 항목 상태 전이 (§5)
        │
        ▼
 SessionView ──emit("session")──► overlay.js
        │
 (사용 가능 + 20초) ─► BootRecord 생성 ─► advice ─► history.jsonl 추가
                                       └─► 로그온 작업 캐시 갱신(Windows)
                                       └─► 조회 중지
```

## 5. 판정 엔진 상태 전이

```
          실행 감지                 조용 5초(최소 관찰 3초)
 Waiting ─────────► Running ──────────────────────────► Settled
   │                  │  ▲                                 │
   │ t≥45s            │  └──── CPU≥20% 3초 (선언 전만) ─────┘
   ▼                  │ 실행 후 60초
 NotLaunched          ▼                트리 전체 종료
                   Persistent        Running ─────► Settled(exited)

 Untrackable : 실행 파일을 특정할 수 없는 항목 (처음부터)
```

- "조용" 판정(최근 5초 창): 트리 CPU 평균 < 8.0, 트리 디스크 평균 < 1,048,576 B/s, 창 안에 새 자식 없음
- Settled 시각은 조건을 처음 만족한 5초 창의 시작으로 기록한다(판정이 확정된 시각이 아님).
- 직전 부팅 기록에서 `not_launched`였던 항목은 `optional`로 시작해 Waiting이어도 판정을 막지 않는다.
- 사용 가능 시각 = 체크리스트가 마지막으로 모두 충족되기 시작한 시각. 이 상태가 3초 유지되면 확정한다.

### 5.1 항목과 프로세스의 연결
1. 등록 항목: 실행 파일 경로(소문자)가 같은 프로세스 중 기준 시각 − 120초 이후 시작된 것을 루트로 잡는다. 경로 없이 파일명만 있는 항목은 파일명으로 비교한다.
2. 트리: 매 조회에서 부모 PID가 항목의 기존 PID 집합(종료된 PID 포함)에 있으면 집합에 추가한다. 부모가 먼저 종료돼도 연결이 유지된다.
3. 자동 감지: t ≤ 45초 구간에 새로 보인 프로세스 중 아래를 모두 만족하면 실행 파일별로 항목을 만든다.
   - 시작 시각 ≥ 기준 시각 − 5초
   - 어떤 항목 트리에도 속하지 않음, 앱 자신 아님
   - 실행 파일이 시스템 경로 밖 (Windows `C:\Windows\`, Mac `/System/`, `/usr/`, `/bin/`, `/sbin/`, `/Library/Apple/`)
   - 부모가 셸(Windows `explorer.exe`, Mac `launchd` PID 1)이거나 이미 종료됨
   - 사용자 ID가 앱과 같음(조회 가능한 경우)

## 6. 화면

### 6.1 오버레이 (360×420, 주 모니터 작업 영역 오른쪽 아래, 여백 16px)
```
┌────────────────────────────────┐
│ ● 얼음!              00:23      │
│ ███████████░░░░░  7 / 12       │
│ ✔ 바탕화면  ◌ 시작 항목  ✔ 네트워크 │
├────────────────────────────────┤
│ ◌ Teams          12%   0:21    │
│ ◌ OneDrive        4%   0:19    │
│ ✔ Slack                0:09    │
│ …                              │
├────────────────────────────────┤
│ 백그라운드: Defender, 인덱서     │
│                     [자세히]    │
└────────────────────────────────┘
```
사용 가능 후: 상단이 "땡!"(초록, 한 번 튀는 애니메이션)으로 바뀌고 아래 줄에 "바탕화면 후 38초 · 등급 B"를 보여주며, 추천 1건 표시, 15초 뒤 숨김(마우스가 올라가 있으면 유지).

### 6.2 상세 (820×640, 일반 창)
1. 요약 카드: 사용 가능 시간, 등급, 부팅 종류, 전원~바탕화면 시간, 보조 지표, 자체 CPU
2. 타임라인(SVG): 항목별 막대(실행~안정), 상태 색, 사용 가능 시점 세로선, 네트워크 연결 표시
3. 추천 목록
4. 이력 표(최근 30회)
5. 설정: 자동 시작 상태·방식

## 7. Tauri 명령
| 명령 | 반환 | 용도 |
|---|---|---|
| `get_view` | `SessionView?` | 오버레이 첫 렌더 |
| `get_history` | `BootRecord[]` | 상세 화면 |
| `get_autostart` | `{enabled, method}` | 상세 설정 |
| `set_autostart(enabled)` | `{enabled, method}` | 토글 |
| `open_details` | — | 오버레이 → 상세 |
| `hide_overlay` | — | 자동 숨김 |
| `start_demo` | — | 데모 측정 |
| `get_update` | `UpdateView` | 현재 버전·새 버전·확인/설치 중·측정 중·오류 |
| `check_update(force)` | — | 확인 요청 (force=false면 6시간 간격, 측정 중이면 무시) |
| `install_update` | `Result` | 받아 둔 새 버전 설치 후 재실행 |

이벤트: `session` (SessionView, 1초마다), `history-updated`, `update-state` (UpdateView).

배포 파일: `.github/workflows/release.yml`(태그 → 초안 릴리스 → Windows·Mac 빌드 → 게시), `src-tauri/windows/hooks.nsh`(제거 훅), `src-tauri/tauri.windows.conf.json`(NSIS만).

## 8. 진행 단계
| Phase | 내용 | 완료 기준 |
|---|---|---|
| 0 | 환경·골격 | `cargo check` 통과 |
| 1 | 판정 엔진·추천 | 단위 테스트 통과 |
| 2 | 수집기·시작 항목·자동 시작 | Windows에서 항목 목록 출력 확인 |
| 3 | 세션·UI | 데모 측정 실행 확인 |
| 4 | 빌드 | `tauri build` 설치 파일 생성 |
| 5 | 실측 조정 | 실제 재부팅 측정 후 수치 검토 (사용자) |
| 6 | Mac 검증 | Mac 실기 빌드·실행 (사용자) |
| 7 | 업데이트 | v0.3.0 릴리스, 0.3.0 → 0.3.1 업데이트 실기 확인 |
