# Phase 7 — 업데이트 (v0.3.0)

| 항목 | 내용 |
|---|---|
| 일자 | 2026-11-02 |
| 근거 | 사용자 요청 "업데이트 기능도", 제안 U1~U6 승인 |
| 요구서 | v1.3 (F11, D9) |

## 결정 (제안 U1~U6)
| # | 항목 | 결정 |
|---|---|---|
| U1 | 서명 키 | 땡해줘 전용 키 새로 생성 (`~/.tauri/ttaeng.key`, `.password`, `.pub`) |
| U2 | 배포 위치 | `lsft32/ttaeng` 릴리스, endpoint `releases/latest/download/latest.json` |
| U3 | 빌드 | GitHub Actions, Windows NSIS + macOS universal (`tauri-action`) |
| U4 | 확인 시점 | 측정 끝난 뒤·상세 화면 볼 때(6시간 간격), 수동 확인은 제한 없음. 측정 중 확인 안 함 |
| U5 | 설치 | 사용자가 누를 때만, Windows passive, 끝나면 재실행. 측정 중 버튼 막음 |
| U6 | MSI | 제외 (`tauri.windows.conf.json` → NSIS만) |

## 구현
- `update.rs`: `UpdateState`(화면 상태 + 받아 둔 `Update`), `check(force)`, `install()`, `notify()`, 순수 함수 `due()`
- `session.rs`: 측정 시작·종료 때 `notify`, 종료 후 `check(false)`
- `lib.rs`: updater 플러그인, 명령 `get_update`·`check_update`·`install_update`, 트레이 "업데이트 확인" ↔ "업데이트 설치 (버전)"
- `history.rs`: `Settings.last_update_check`
- `ui/details.*`: 헤더에 현재 버전·"업데이트 확인" 버튼, 위쪽 배너(새 버전·설치 중·실패)
- `.github/workflows/release.yml`: 초안 릴리스 생성 → Windows·Mac 빌드(`npm test` 포함) → 둘 다 성공하면 게시

## 검증
- `cargo test` 27개 통과 (추가: `auto_check_interval`)
- 로컬 릴리스 빌드(서명 키 환경변수): `땡해줘_0.3.0_x64-setup.exe`(2.24 MiB) + `.exe.sig` 생성
- 실행 확인: 상세 화면 헤더에 `v0.3.0`·"업데이트 확인" 버튼 표시. 시작 직후 자동 확인이 돌아 `last_update_check` 기록됨. 아직 릴리스가 없어 실패했지만 자동 확인이라 배너를 띄우지 않음(의도대로)

## 남은 것
- [ ] GitHub Secrets 등록 (자동 모드 권한 정책으로 차단 → 사용자가 직접 실행)
- [ ] 커밋·푸시 → 태그 `v0.3.0` push → Actions 결과 확인 (Mac 빌드는 여기서 처음 컴파일됨)
- [ ] 0.3.0 직접 설치 → 0.3.1 릴리스 → 앱에서 업데이트 설치 확인
