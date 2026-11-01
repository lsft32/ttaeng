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

## v0.3.0 릴리스 (2026-11-02)
- GitHub Secrets 등록: 자동 모드 권한 정책에 막혀 사용자가 직접 실행
- 1차(run 41321320122) 실패: Mac에서 단위 테스트 `hosts_are_untrackable` 실패. Windows 경로(`C:\Windows\System32\rundll32.exe`)를 Mac은 백슬래시로 나누지 않음
  - Mac 컴파일 자체는 처음으로 성공(경고 1건: Windows 전용 import)
  - 조치: 테스트를 OS별 경로로, import를 `cfg(windows)`로 (ce335ab). 초안 릴리스·태그 삭제 후 같은 태그로 재실행(사용자 승인)
- 2차(run 41321800800) 빌드·게시 성공, 그러나 **latest.json 누락**
  - 원인: GitHub가 릴리스 파일 이름의 한글을 지움("땡해줘_0.3.0_x64-setup.exe" → "_0.3.0_x64-setup.exe"). tauri-action이 서명 파일을 이름으로 찾지 못해 "Signature not found for the updater JSON. Skipping upload"
  - v0.3.0 조치(수동): 파일 이름을 `ttaeng_...`로 변경(API), `scripts/release-assets.mjs latest`로 latest.json 생성·업로드. endpoint에서 0.3.0·3개 플랫폼 확인, 설치 파일 URL 200
  - 이후 릴리스: tauri-action 대신 `npx tauri build` → `release-assets.mjs collect`(ASCII 이름) → 게시 job에서 `latest` 생성 후 `gh release create`로 한 번에 게시. 릴리스 노트는 주석 태그 메시지(없으면 커밋 제목)

## 업데이트 실기 확인
- 릴리스의 `ttaeng_0.3.0_x64-setup.exe`를 0.2.0 위에 설치(`/S`): 0.3.0으로 바뀜, Run 키·settings.json 유지
- v0.3.1: 버전만 올린 확인용 릴리스(바뀐 워크플로 첫 실행)

## 남은 것
- [ ] 앱에서 0.3.0 → 0.3.1 업데이트 설치 확인
