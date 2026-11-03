# 땡해줘 — 데이터 스키마

| 항목 | 내용 |
|---|---|
| 문서 버전 | 1.0 |
| 작성일 | 2026-11-01 |

DB는 쓰지 않고 앱 데이터 폴더(Tauri `app_data_dir`)의 파일 3개를 쓴다.
- Windows: `%APPDATA%\com.lsft32.ttaeng\`
- Mac: `~/Library/Application Support/com.lsft32.ttaeng/`

| 파일 | 형식 | 내용 |
|---|---|---|
| `history.jsonl` | JSON Lines | 부팅 기록 1회 = 1줄, 최근 200줄 유지 |
| `settings.json` | JSON | 설정 |
| `logon_tasks.json` | JSON | Windows 로그온 예약 작업 캐시 |

시각은 모두 Unix epoch 초(정수), 경과 시간은 기준 시각 대비 초(실수 소수 1자리).

## 1. BootRecord (`history.jsonl` 한 줄)

| 필드 | 타입 | 설명 |
|---|---|---|
| `v` | int | 스키마 버전 (1, v1.5부터 2: 10분 관찰 기록) |
| `os` | string | `windows` / `macos` |
| `boot_source` | string | `event`(Kernel-Boot 27) / `estimate`(10분 규칙) / `sysinfo`(Mac) / 빈 값(데모·v1.3 이전), v1.4 |
| `power_to_logon_s` | float? | 부팅 → 로그온(Winlogon 7001), 로그인 화면 대기 포함, v1.4 |
| `logon_to_desktop_s` | float? | 로그온 → 바탕화면, v1.4 |
| `measure_start_s` | float | 측정(첫 조회) 시작 시각, 기준 대비 초, v1.4 |
| `late_start` | bool | 측정 시작이 30초 이상 늦음 → 결과 참고용, v1.4 |
| `network_before_start` | bool | 측정 시작 시 이미 연결 → `network_s`는 null, v1.4 |
| `observe_until_s` | float | 세션 종료 시각(기준 대비 초), v1.5 |
| `all_launched_s` | float? | 마지막으로 실행된 시작 항목의 실행 시각, v1.5 |
| `refreeze` | `{key, name, from_s, to_s}[]` | 땡! 이후 다시 얼음!이 된 구간, v1.5 |
| `boot_kind` | string | `cold` / `fast_startup` / `resume`(최대 절전 복귀, v1.4) / `demo` |
| `boot_time` | int | OS 부팅 시각 (Windows v1.4: Kernel-Boot 27 이벤트 시각, 못 읽으면 sysinfo) |
| `anchor_time` | int | 기준 시각 (바탕화면 표시) |
| `power_to_desktop_s` | float | 부팅~기준 시각 (Windows는 로그인 대기 포함) |
| `event100` | object? | `{main_path_ms, post_boot_ms, boot_ms}`, 읽을 수 없으면 null |
| `ready_s` | float | 사용 가능까지 (t) |
| `forced` | bool | 5분 상한으로 강제 판정했는지 |
| `grade` | string | A/B/C/D |
| `network_s` | float? | 네트워크 연결 시점, 실패 시 null |
| `aux_idle_s` | float? | 보조 지표 달성 시점 |
| `self_cpu_avg` | float | 측정 중 앱 자신의 평균 CPU (코어 1개 기준 %) |
| `bottleneck` | string? | 가장 늦게 안정된 항목 키 |
| `items` | ItemRecord[] | 항목별 결과 |
| `background` | string[] | 관찰된 백그라운드 작업 이름 |
| `advice` | Advice[] | 추천 |

### ItemRecord
| 필드 | 타입 | 설명 |
|---|---|---|
| `key` | string | 식별자: 실행 파일 경로 소문자(없으면 `name:<이름>`) |
| `name` | string | 표시 이름 |
| `source` | string | `run_key` / `startup_folder` / `logon_task` / `launch_agent` / `detected` |
| `state` | string | `settled` / `exited` / `persistent` / `not_launched` / `untrackable` |
| `launch_s` | float? | 실행 감지 시점 (기준 이전 실행이면 음수 가능) |
| `settle_s` | float? | 안정 시점 |
| `late` | bool | 45초 이후 실행(늦은 실행), v1.5 |
| `delay_s` | float | 예약 작업 로그온 지연(초), 45초 이상이면 예약 지연, v1.6 |
| `idle_only` | bool | 예약 작업이 유휴 시에만 실행, v1.6 |
| `missing` | bool | 등록된 실행 파일 없음, v1.6 |
| `peak_cpu` | float | 최대 트리 CPU |
| `cpu_seconds` | float | 트리 CPU 누적 (코어·초) |
| `disk_mb` | float | 트리 디스크 누적 MB |

### Advice
| 필드 | 타입 | 설명 |
|---|---|---|
| `rule` | string | `R1`~`R5` |
| `title` | string | 한 줄 요약 |
| `detail` | string | 근거 수치와 방법 |
| `item` | string? | 관련 항목 키 |

## 2. settings.json
| 필드 | 타입 | 기본값 | 설명 |
|---|---|---|---|
| `autostart_initialized` | bool | false | 첫 실행 자동 등록을 했는지 |
| `last_update_check` | int | 0 | 마지막 업데이트 확인 시각(epoch 초). 0이면 확인한 적 없음 |

제거(업데이트 아님) 시 이 파일을 지운다. 다시 설치하면 첫 실행에서 자동 시작을 다시 등록한다.

## 3. logon_tasks.json (Windows)
| 필드 | 타입 | 설명 |
|---|---|---|
| `updated` | int | 갱신 시각 |
| `tasks` | `{name, exe, path, delay_s, idle_only}[]` | 로그온 트리거가 있는 사용 중 작업. `exe`는 환경변수 확장 전 원문. `path`(작업 폴더+이름, 측정 시작 때 작업 파일 존재 확인), `delay_s`(로그온 트리거 지연 최솟값), `idle_only`(유휴 시만 실행)는 v1.6 |

## 4. SessionView (화면 이벤트, 저장 안 함)
| 필드 | 타입 | 설명 |
|---|---|---|
| `demo` | bool | 데모 측정 여부 |
| `t` | float | 경과 초 |
| `phase` | string | `preparing` / `ready` / `watching`(땡! 이후 늦은 항목 관찰, v1.5) / `done` |
| `ready_s` | float? | 사용 가능 시점 |
| `forced` | bool | |
| `grade` | string? | |
| `checklist` | `{desktop, items, network}` | 각 `ok` / `wait` / `skip` |
| `network_note` | string | `연결됨` / `확인 중` / `네트워크 없음` |
| `progress` | `{done, total}` | |
| `items` | `{key, name, source, state, cpu, launch_s, settle_s}[]` | `state`는 ItemRecord + `waiting` / `running` / `deferred`(늦게 실행 대기, v1.5) |
| `background` | `{name, cpu}[]` | |
| `top_advice` | Advice? | 사용 가능 후 1건 |
