# Phase 0~3 — 환경·판정 엔진·수집기·UI

| 항목 | 내용 |
|---|---|
| 기간 | 2026-11-01 (하루) |
| 상태 | **완료** |

## Phase 0 — 환경
| 도구 | 버전 |
|---|---|
| Rust / cargo | 1.95.0 |
| Node.js / npm | 24.14.1 / 11.11.0 |
| Tauri | 2.12.5 (CLI는 devDependency) |
| sysinfo | 0.40.1 (`system` 기능만) |
| winreg / plist | 0.56.0 / 1.x |

- 아이콘: `src-tauri/icons/source.svg` → `npx tauri icon`으로 생성(모바일 아이콘 폴더는 삭제)
- 사전 확인
  - Event 100 읽기: 일반 권한에서 `Access is denied` → 선택 기능으로 둠
  - 로그온 예약 작업 조회(`Get-ScheduledTask`) 3.3초 → 부팅 중 조회하지 않고 세션 종료 후 캐시

## Phase 1 — 판정 엔진·추천 (`judge.rs`, `advice.rs`)
- 상태 전이: Waiting → Running → Settled / Exited / Persistent, NotLaunched, Untrackable
- 프로세스 트리: 종료된 부모 pid도 남겨 고아 자식 연결, pid 재사용은 시작 시각으로 구분
- 자동 감지: 셸이 띄운 비시스템 프로세스(기준 시각 −5초 이후, 45초 이내)
- 단위 테스트 25개 통과 (`cargo test --lib`)

## Phase 2 — 수집기·시작 항목·자동 시작
- `collector.rs`: sysinfo 1초 조회 (CPU·디스크·exe·부모·시작 시각·사용자)
- `startup/windows.rs`: Run 키 3곳 + StartupApproved, 시작 폴더 2곳(.lnk 직접 파싱), 로그온 작업 캐시, Event 100
- `startup/macos.rs`: LaunchAgents(RunAtLoad) — **컴파일·실행 미검증**
- `autostart.rs`: Windows 작업 스케줄러 → 실패 시 HKCU Run, Mac LaunchAgent
- 검증
  - 이 PC Run 키 23개 경로 해석 정상
  - `.lnk` 파서: 시작 메뉴 바로가기 8개 대상 경로가 `WScript.Shell` 결과와 전부 일치(한글 파일명 포함)
  - 꺼 둔 시작 폴더 항목(`SIDEY 단축키.lnk`, StartupApproved 첫 바이트 03) 제외 확인
  - 로그온 작업 캐시 7건 저장 확인(`%APPDATA%\com.lsft32.settle\logon_tasks.json`)

## Phase 3 — 세션·UI
- `session.rs`: 1초 루프, 판정 후 20초 관찰 → 기록 저장 → 조회 중지 → 작업 캐시 갱신
- `ui/overlay.*`: 오른쪽 아래 패널(포커스 안 가져감), `ui/details.*`: 요약·타임라인(SVG)·추천·이력
- 동작 확인용 `SETTLE_DEMO=1` 환경변수 → 데모 측정으로 시작
- 데모 실행 확인(디버그 빌드, 창 캡처)
  - 준비 중 → 사용 가능(등급) → 추천 1건 표시까지 정상
  - 두 번째 실행 시 상세 창 열림(single-instance) 정상

### 실측으로 바뀐 것 (요구서 v1.1 §9)
1. 1차 데모: RaiDrive.Mount가 대기 중에도 CPU를 써서 3% 기준을 끝내 못 넘고 60초 지속형 → 병목 처리됨
2. 원인: Windows CPU 시간 15.6ms 단위 집계로 1초 표본이 계단식(0/1.6/3.1/4.7%)이고 가끔 17~25% 튐
3. 조치: CPU·디스크·자식 조건을 5초 창 평균으로 변경, CPU 기준 8%
4. 결과: 실행 중 항목 전부 안정까지 37초 → 11초

## 특이사항
- 빌드할 때마다 AhnLab V3가 새 `settle_lib.dll`/테스트 exe에 대해 "클라우드 자동 분석 요청" 창을 띄움. 사용자 PC 보안 프로그램의 판단 영역이라 조작하지 않음
- 판정 후 오버레이는 등급 → (20초 후) 추천 표시 → 15초 뒤 숨김으로 동작(요구서 F4-6 v1.1 반영)
