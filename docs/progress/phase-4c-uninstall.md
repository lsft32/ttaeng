# Phase 4c — 제거 시 자동 시작 정리

| 항목 | 내용 |
|---|---|
| 일자 | 2026-11-02 |
| 근거 | 사용자 요청 — 제거할 때 Run 키도 지우기 |

## 조사
- Tauri NSIS 기본 제거 단계에 이미 `DeleteRegValue HKCU "...\Run" "${PRODUCTNAME}"`가 있음(업데이트 모드 제외). 앱이 쓰는 Run 값 이름이 `땡해줘`(= productName)라 그대로 지워짐
- 빠진 것
  - 작업 스케줄러 작업 `땡해줘`(관리자 권한으로 실행해 등록된 경우)
  - `settings.json`의 `autostart_initialized`가 남아, 다시 설치해도 자동 시작이 재등록되지 않음

## 변경
- `src-tauri/windows/hooks.nsh` 추가, `bundle.windows.nsis.installerHooks`로 연결
- `NSIS_HOOK_POSTUNINSTALL`: 업데이트(`/UPDATE`)가 아닐 때만
  - `schtasks /Delete /TN "땡해줘" /F`
  - `%APPDATA%\com.lsft32.ttaeng\settings.json` 삭제 (부팅 이력은 남김. 앱 데이터 삭제는 기존 체크박스 몫)
- MSI는 손대지 않음 (업데이트 기능 제안 U6에서 Windows 배포를 NSIS만으로 줄이는 안 검토 중)

## 검증 (이 PC, 사용자 단위 설치)
| 시점 | Run 키 | 예약 작업 | settings.json | 실행 파일 |
|---|---|---|---|---|
| 제거 전 | 있음 | 있음(시험용으로 같은 이름 작업 생성) | 있음 | 있음 |
| 제거 후(`uninstall.exe /S`) | 없음 | 없음 | 없음 | 없음 |
| 재설치 후 첫 실행 | 다시 등록됨 | — | `autostart_initialized: true` | 있음 |

## 첫 설치 실행 결과 (Phase 4 미확인 항목)
- 일반 권한 첫 실행에서 작업 스케줄러 등록은 실패, **HKCU Run 키로 등록**됨 (상세 화면 표시 "Run 키(지연 실행)")
