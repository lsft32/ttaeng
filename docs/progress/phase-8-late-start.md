# Phase 8 — 측정 지연·부팅 종류 보정 (v0.3.2)

| 항목 | 내용 |
|---|---|
| 일자 | 2026-11-02 |
| 근거 | 첫 실제 부팅 기록(2026-11-02 09:01, 빠른 시작) 검토, 사용자 승인 F1~F5 |
| 요구서 | v1.4 (F1-2, F1-5, F7 R0·R5, F8, §9) |

## 첫 기록에서 드러난 문제
| 시각(바탕화면 기준) | 내용 |
|---|---|
| 0초 (09:01:46) | explorer 시작 |
| 3 ~ 220초 | 다른 시작프로그램 실행. 63초 이후 약 30초 간격(129·159·190·220초), 원인 미확인 |
| 250초 (09:05:56) | 땡해줘 실행 (Run 키, 줄 끝) |

- R5 "네트워크 257.1초": 앱 시작 후 첫 확인 성공 시각이 기록된 것
- R1 "TrafficMonitor가 259.1초를 정함": 3초 실행 항목이 처음 관찰된 시각(약 259초)에 지속형이 됐다고 기록한 버그
- 등급 D: 늦은 시작 때문에 판정이 "앱 시작 + 약 9초"보다 빨라질 수 없음
- "전원 → 바탕화면 66,386초": 빠른 시작에서 sysinfo 부팅 시각이 전날 콜드 부팅(14:35)으로 남음

## 변경
| # | 내용 | 파일 |
|---|---|---|
| F1 | 작업 스케줄러 **현재 사용자 로그온** 트리거(XML, 관리자 불필요). 배터리에서도 실행·시간 제한 없음·우선순위 4. Run 키로 등록돼 있으면 실행 시 자동 이전. 탐색기가 없으면 최대 60초 대기 후 기준 시각 결정 | `autostart.rs`, `lib.rs`, `session.rs` |
| F2 | 지속형 충족 시각 = 실행 + 60초 | `judge.rs` |
| F3 | 첫 확인에서 이미 연결이면 `network_before_start`, 기록 `network_s` null, 판정엔 0초로 넘김 → R5 안 냄 | `network.rs`, `session.rs` |
| F4 | 측정 시작 30초 이상 늦으면 `late_start`: R0 추가, R1·R4 제외, 병목 미기록, 등급 "참고" 표시, 오버레이에 "측정 N초 늦게 시작" | `advice.rs`, `session.rs`, `ui/*` |
| F5 | Kernel-Boot 27(부팅 유형·시각), Winlogon 7001(로그온 시각)로 부팅 종류와 "부팅 → 로그온 / 로그온 → 바탕화면" 카드. 못 읽으면 기존 추정 | `startup/windows.rs`, `session.rs`, `ui/details.js` |

## 검증
- 일반 권한 확인: "모든 사용자 로그온"(`schtasks /SC ONLOGON`)은 `Access is denied`, **현재 사용자 로그온 트리거는 등록 성공**(PowerShell·`schtasks /XML` 둘 다, 시험 작업은 바로 삭제)
- 이 PC 이벤트 읽기: `BootInfo { boot_time: 09:01:43, boot_type: 1(빠른 시작), logon_time: 09:01:44 }`
- `cargo test` 33개 통과 (추가: `late_observation_keeps_persistent_time`, `connected_before_measuring_is_unknown`, `boot_info_from_events`, `real_boot_info_reads`, `late_start_is_reference_only`, `task_xml_is_user_scoped_and_escaped`)

- 디버그 빌드 상세 화면: 헤더 `v0.3.2`, 이전(v1.3) 기록의 부팅 종류가 "빠른 시작(추정)"으로 표시. 새 카드·"참고" 표시는 새 기록이 있어야 보여 미확인
- 로컬 릴리스 빌드(서명): `땡해줘_0.3.2_x64-setup.exe` + `.sig` 생성

## 남은 것
- [ ] 0.3.2 설치 후 작업 스케줄러로 이전됐는지(상세 화면 "작업 스케줄러" 표시)
- [ ] 재부팅 후 땡해줘가 바탕화면 직후 시작하는지, 측정 결과 재검토
