# Phase 4 — 릴리스 빌드

| 항목 | 내용 |
|---|---|
| 일자 | 2026-11-01 |
| 상태 | **완료** (설치·실부팅 측정은 사용자 확인 대기) |

## 결과
`npm run build` 성공 (Rust 릴리스 컴파일 4분 25초, 번들 대상 `all`)

| 산출물 | 경로 | 크기 |
|---|---|---|
| NSIS 설치 파일 | `src-tauri/target/release/bundle/nsis/Settle_0.1.0_x64-setup.exe` | 1.43 MiB |
| MSI | `src-tauri/target/release/bundle/msi/Settle_0.1.0_x64_en-US.msi` | 2.10 MiB |
| 실행 파일 | `src-tauri/target/release/settle.exe` | — |

- WiX 3.14, NSIS 3.13, nsis_tauri_utils 0.5.3은 빌드 중 자동 다운로드(해시 검증)
- NSIS는 `currentUser` 설치(관리자 권한 불필요)

## 미확인
- [x] 설치 후 첫 실행 시 자동 시작 등록 방식 → 일반 권한에서는 Run 키 (2026-11-02, phase-4c)
- [ ] 실제 재부팅 측정 1회 이상 → 수치 검토(Phase 5)
- [ ] Mac 빌드·실행(Phase 6)
