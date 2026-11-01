# Settle

PC를 켜고 로그인한 뒤 **언제부터 써도 되는지**를 실시간으로 보여주는 데스크톱 도구입니다. 시작프로그램이 하나씩 정리되는 과정을 오른쪽 아래 패널에 표시하고, 끝나면 사용 가능 시간·등급·추천을 남깁니다.

- 플랫폼: Windows 10/11 (검증), macOS 13+ (코드만 작성, 미검증)
- 스택: Tauri 2 + Rust(sysinfo), 프런트는 빌드 도구 없는 HTML/JS
- 문서: `docs/01-requirements.md`(판정 규칙·수치), `docs/02-prd.md`(구조), `docs/03-data-schema.md`(저장 형식), `docs/progress/`

## 명령 (프로젝트 루트)

```powershell
npm ci                 # Tauri CLI 설치
npm test               # 단위 테스트 (cargo test)
npm run dev            # 개발 실행
npm run build          # 설치 파일 생성 → src-tauri/target/release/bundle/
```

데모 측정으로 바로 시작하기(부팅 직후가 아니어도 화면 확인 가능):

```powershell
$env:SETTLE_DEMO = '1'; .\src-tauri\target\debug\settle.exe
```

## 동작 요약
1. 로그인 시 자동 실행(Windows: 작업 스케줄러 → 실패 시 Run 키, Mac: LaunchAgent). 릴리스 빌드 첫 실행 때 자동 등록
2. 바탕화면 표시 시각(explorer / Dock 시작 시각)을 기준(t=0)으로 1초마다 시작 항목 프로세스 트리를 관찰
3. 필수 체크리스트(바탕화면·시작 항목 전부 정리·네트워크)가 3초 유지되면 "사용 가능"
4. 20초 더 관찰 후 기록 저장(`%APPDATA%\com.lsft32.settle\history.jsonl`), 조회 중지
5. 부팅 후 10분이 지나 실행하면 측정하지 않고 상세 화면(이력)을 연다

시작 항목을 자동으로 끄거나 바꾸지 않습니다. 추천은 끄는 방법만 안내합니다.
