# 땡해줘

PC를 켜고 로그인한 뒤 **언제부터 써도 되는지**를 실시간으로 보여주는 데스크톱 도구입니다. 얼음땡에서 "땡"을 해 줘야 움직일 수 있듯, 시작프로그램이 정리되는 동안은 오른쪽 아래 패널에 **얼음!**, 다 끝나면 **땡!**을 띄우고 사용 가능 시간·등급·추천을 남깁니다.

- 플랫폼: Windows 10/11 (검증), macOS 13+ (코드만 작성, 미검증)
- 스택: Tauri 2 + Rust(sysinfo), 프런트는 빌드 도구 없는 HTML/JS
- 문서: `docs/01-requirements.md`(판정 규칙·수치), `docs/02-prd.md`(구조), `docs/03-data-schema.md`(저장 형식), `docs/progress/`
- 이름: 화면·설치 이름은 `땡해줘`, 실행 파일·크레이트는 `ttaeng`, 식별자는 `com.lsft32.ttaeng`

## 명령 (프로젝트 루트)

```powershell
npm ci                 # Tauri CLI 설치
npm test               # 단위 테스트 (cargo test)
npm run dev            # 개발 실행
npm run build          # 설치 파일 생성 → src-tauri/target/release/bundle/
```

데모 측정으로 바로 시작하기(부팅 직후가 아니어도 화면 확인 가능):

```powershell
$env:TTAENG_DEMO = '1'; .\src-tauri\target\debug\ttaeng.exe
```

## 릴리스 (업데이트 배포)

1. `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `package.json`의 버전을 올리고 커밋
2. 태그를 push하면 GitHub Actions(`.github/workflows/release.yml`)가 Windows·Mac을 빌드·서명해 릴리스에 올린다

```powershell
git tag -a v0.3.1 -m "바뀐 내용 한두 줄"; git push origin v0.3.1
```

- 태그 메시지가 릴리스 노트이자 앱 업데이트 배너에 보이는 문구가 된다
- 릴리스 파일 이름은 `ttaeng_...`(GitHub가 한글을 지우므로 `scripts/release-assets.mjs`가 바꾼다)

- 설치된 앱은 측정이 끝난 뒤 6시간 간격으로 `latest.json`을 확인하고, 새 버전이 있으면 상세 화면·트레이에 알린다. 설치는 사용자가 누를 때만 한다
- 서명 개인키: 개발 PC `~/.tauri/ttaeng.key`(+ `.password`)와 저장소 Secrets `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. **키를 잃어버리면 기존 설치본은 더 이상 업데이트를 받을 수 없다**
- 로컬에서 `npm run build`를 하려면 위 두 값을 환경변수로 넣어야 한다(업데이트 서명 파일 생성)

## 동작 요약
1. 로그인 시 자동 실행(Windows: 작업 스케줄러 → 실패 시 Run 키, Mac: LaunchAgent). 릴리스 빌드 첫 실행 때 자동 등록
2. 바탕화면 표시 시각(explorer / Dock 시작 시각)을 기준(t=0)으로 1초마다 시작 항목 프로세스 트리를 관찰. 이 동안 **얼음!**
3. 필수 체크리스트(바탕화면·시작 항목 전부 정리·네트워크)가 3초 유지되면 **땡!**(사용 가능)
4. 20초 더 관찰 후 기록 저장(`%APPDATA%\com.lsft32.ttaeng\history.jsonl`), 조회 중지
5. 부팅 후 10분이 지나 실행하면 측정하지 않고 상세 화면(이력)을 연다

시작 항목을 자동으로 끄거나 바꾸지 않습니다. 추천은 끄는 방법만 안내합니다.
