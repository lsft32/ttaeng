# Phase 4b — 이름 변경(땡해줘)·얼음/땡 표시 (v0.2.0)

| 항목 | 내용 |
|---|---|
| 일자 | 2026-11-01 |
| 근거 | 사용자 요청 — 이름 "땡해줘", 쓰면 안 되는 동안 "얼음!", 끝나면 "땡!" |
| 요구서 | v1.2 (F4-2, F9, D8, §9) |

## 변경
- 이름
  - 화면·설치 이름(`productName`, 창 제목, 트레이, 자동 시작 등록 이름): `땡해줘` → `platform::APP_NAME` 상수 하나로 관리
  - 실행 파일·크레이트·npm 패키지: `ttaeng` (`mainBinaryName`, `ttaeng_lib`)
  - 식별자·데이터 폴더·Mac LaunchAgent 라벨: `com.lsft32.ttaeng`
  - 데모 환경변수: `TTAENG_DEMO`
  - MSI는 한글 제품명 때문에 WiX 언어를 `ko-KR`(코드페이지 949)로 지정
  - 프로젝트 폴더명: `dev_project/settle` → `dev_project/ttaeng` (같은 날 사용자 요청)
- 얼음/땡
  - 오버레이 제목: 판정 전 "얼음!"(하늘색, 가끔 떨림), 판정 후 "땡!"(초록, 한 번 튐). 타임아웃으로 빠진 항목이 있으면 "일부 지연" 표시
  - 진행 막대·상태 점도 판정 전에는 하늘색
  - 트레이 툴팁: "땡해줘 · 얼음!" / "땡해줘 · 땡!" (상태가 바뀔 때만 갱신)
  - 움직임 줄이기 설정(`prefers-reduced-motion`)이면 애니메이션 없음
- 기타: 상태 "안정" 툴팁의 기준 문구를 v1.1 수치(5초 평균 8%)로 고침

## 검증
- `cargo test` 26개 통과 (추가: `freeze_label_follows_ready`)
- 디버그 빌드 데모 측정 창 캡처
  - 3초: "얼음!" 하늘색, 0/23 정리
  - 63초: "땡!" 초록, 바탕화면 후 45.0초·등급 B
  - 45초가 걸린 이유: 데이터 폴더가 새로 바뀌어 직전 기록이 없으므로 미실행 항목(KakaoTalk 등)을 45초까지 기다림(규칙대로). 두 번째 측정부터는 생략됨
- 트레이 툴팁은 화면으로 확인하지 않음(라벨 함수 테스트만)
- `npm run build` 성공: 한글 제품명으로 NSIS·MSI 모두 생성
  - `src-tauri/target/release/bundle/nsis/땡해줘_0.2.0_x64-setup.exe` (1.43 MiB)
  - `src-tauri/target/release/bundle/msi/땡해줘_0.2.0_x64_ko-KR.msi` (2.11 MiB)

## 남은 것
- 이전 데이터 폴더 `%APPDATA%\com.lsft32.settle\`(개발 중 생긴 `logon_tasks.json` 1개)는 지우지 않고 둠
