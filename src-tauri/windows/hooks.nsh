; 땡해줘 NSIS 설치 프로그램 훅 (tauri.conf.json bundle.windows.nsis.installerHooks)
; Run 키 값(${PRODUCTNAME})은 Tauri 기본 제거 단계가 지운다. 여기서는 그 밖의 것을 정리한다.

!macro NSIS_HOOK_POSTUNINSTALL
  ; 업데이트(/UPDATE)로 인한 제거일 때는 자동 시작과 설정을 그대로 둔다.
  ${If} $UpdateMode <> 1
    ; 관리자 권한으로 실행해 작업 스케줄러에 등록된 경우
    nsExec::Exec 'schtasks /Delete /TN "${PRODUCTNAME}" /F'
    Pop $0
    ; 다시 설치했을 때 첫 실행에서 자동 시작을 다시 등록하도록 초기화한다.
    SetShellVarContext current
    Delete "$APPDATA\${BUNDLEID}\settings.json"
  ${EndIf}
!macroend
