# Phase 10 — 예약 지연·유휴 조건·파일 없음 (v0.3.4)

| 항목 | 내용 |
|---|---|
| 일자 | 2026-11-03 |
| 근거 | 사용자 보고 "삭제한 프로그램을 계속 기다리는 것 같다", 제안 1·2·3 승인 |
| 요구서 | v1.6 (§4.3, F7 R3·R7, §9) |

## 조사 (2026-11-03)
| 시각 | 내용 |
|---|---|
| 09:18 | 빠른 시작 부팅. **0.3.2로 측정**(기록 v1) → 0.3.2의 45초 규칙 때문에 R3 17건(잘못된 추천). 측정 후 0.3.3 업데이트 |
| 09:20~09:27 | 사용자가 R3 목록을 보고 프로그램 삭제·시작 항목 "사용 안 함" |
| 09:27 | 콜드 부팅, **0.3.3 첫 측정**(기록 v2). 600초까지 관찰 |

- 삭제·사용 안 함으로 바꾼 Run 키 항목(CrossEXService, MaEPSBroker, REPORTXSvc, OneDrive, Notion 등)은 측정 대상에서 정상 제외됨(StartupApproved `03` 확인)
- 600초까지 기다린 대상은 예약 작업 2개
  - `OneDrive Startup Task`: 로그온 트리거 지연 `PT10M`, 09:37:23(600초 직후) 실행. Run 키의 OneDrive를 꺼도 이 작업은 남음
  - `OfficeBackgroundTaskHandlerLogon`: `RunOnlyIfIdle=True`, 로그온 때 실행 안 됨
- 첫 v2 측정이라 "직전 미실행 생략" 근거가 없던 것도 원인
- 09:18의 R3 17건 중 대부분이 09:27에 늦게 실행됨(LG 5종 123~143초, vmware-tray 146, AnySign 208, Canon 239·269, Office 업데이트 302) → R3는 잘못된 추천이었음(0.3.3에서 수정된 문제가 업데이트 전 측정에 나온 것)
- 그 외 확인: zzap-easy가 늦은 항목 중 가장 무거움(지속형, CPU 225코어·초, 98~124초 다시 얼음!)

## 변경
| # | 내용 | 파일 |
|---|---|---|
| 1 | 예약 작업 캐시에 `path`·`delay_s`(로그온 트리거 지연 최솟값)·`idle_only` 저장. 지연 45초 이상은 **예약 지연**(판정 비차단, 지연+45초가 10분 안이면 그때까지만 기다림, R6·R3 제외), 유휴 조건은 **유휴 시 실행**(기다리지 않음, R3 제외). 몇 초 지연(LG 배경화면 5초, TrafficMonitor 3초)은 일반 항목 | `startup/windows.rs`, `history.rs`, `judge.rs`, `advice.rs` |
| 2 | 측정 시작 때 캐시의 작업 정의 파일(`%SystemRoot%\System32\Tasks\…`)이 없으면 제외. 측정하지 않는 실행(부팅 10분 후 실행·업데이트 직후)에서도 캐시 갱신 | `startup/windows.rs`, `session.rs` |
| 3 | Run 키·시작 폴더·작업의 실행 파일이 없으면 **파일 없음**: 처음부터 미실행·비차단, 추천 R7 즉시 | `startup/mod.rs`, `judge.rs`, `advice.rs` |
| 화면 | 오버레이 시간 칸·타임라인 미실행 칸에 "예약 지연 N분" / "유휴 시 실행" / "파일 없음" + 툴팁 | `ui/*` |

## 검증
- `cargo test` 44개 통과. 추가: `planned_tasks_do_not_block_or_overextend_watch`, `short_task_delay_is_a_normal_item`, `missing_executable_is_not_waited_for`, `planned_and_missing_items_are_not_r3_and_missing_is_r7`, `missing_executable_is_flagged`, `task_file_path`
- 바꾼 예약 작업 조회 스크립트를 이 PC에서 직접 실행: OneDrive 600초, Office 업데이트 300초, Office Telemetry FallBack 1800초, OfficeBackgroundTaskHandlerLogon 유휴 조건, LG 배경화면 5초·TrafficMonitor 3초 지연 확인
- 작업 정의 파일 존재 여부를 일반 권한으로 확인할 수 있음을 확인

## 남은 것
- [ ] 0.3.4 설치 후 재부팅: OneDrive·Office 작업이 "예약 지연"/"유휴 시 실행"으로 표시되고 측정이 600초 전에 끝나는지
