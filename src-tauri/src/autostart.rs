//! 로그인 자동 시작 등록.
//! Windows는 Run 키가 탐색기 시작 후 지연 실행되므로 작업 스케줄러 "로그온 시"를 먼저 시도한다.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AutoStatus {
    pub enabled: bool,
    /// `task_scheduler` / `run_key` / `launch_agent` / `none`
    pub method: String,
}

#[cfg(windows)]
use crate::platform::APP_NAME as NAME;

fn st(enabled: bool, method: &str) -> AutoStatus {
    AutoStatus { enabled, method: method.into() }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use crate::startup::windows::hidden;
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

    fn ok(args: &[&str]) -> bool {
        hidden("schtasks").args(args).output().map(|o| o.status.success()).unwrap_or(false)
    }

    pub fn status() -> AutoStatus {
        if ok(&["/Query", "/TN", NAME]) {
            return st(true, "task_scheduler");
        }
        let run = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(RUN)
            .and_then(|k| k.get_value::<String, _>(NAME))
            .is_ok();
        if run { st(true, "run_key") } else { st(false, "none") }
    }

    fn esc(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    }

    /// 현재 사용자 로그온 트리거 작업. "모든 사용자"(`/SC ONLOGON`)와 달리 관리자 권한 없이 등록된다.
    /// 기본값으로 두면 배터리에서 실행 안 함·72시간 뒤 종료·낮은 우선순위(7)라서 직접 지정한다.
    pub fn task_xml(user: &str, exe: &str) -> String {
        let (user, exe) = (esc(user), esc(exe));
        format!(
            r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>땡해줘: 로그온 직후 부팅 준비 상태를 측정합니다.</Description></RegistrationInfo>
  <Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId></LogonTrigger></Triggers>
  <Principals><Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>LeastPrivilege</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>4</Priority>
  </Settings>
  <Actions Context="Author"><Exec><Command>{exe}</Command></Exec></Actions>
</Task>
"#
        )
    }

    fn create_task(exe: &str) -> bool {
        let user = match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
            (Ok(d), Ok(u)) => format!("{d}\\{u}"),
            (_, Ok(u)) => u,
            _ => return false,
        };
        // schtasks /XML은 UTF-16(BOM) 파일을 받는다.
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend(task_xml(&user, exe).encode_utf16().flat_map(|c| c.to_le_bytes()));
        let path = std::env::temp_dir().join(format!("ttaeng-task-{}.xml", std::process::id()));
        if std::fs::write(&path, bytes).is_err() {
            return false;
        }
        let done = ok(&["/Create", "/TN", NAME, "/XML", &path.to_string_lossy(), "/F"]);
        let _ = std::fs::remove_file(&path);
        done
    }

    pub fn enable(exe: &str) -> AutoStatus {
        let tr = format!("\"{exe}\"");
        let task = create_task(exe);
        if let Ok((key, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN) {
            if task {
                let _ = key.delete_value(NAME);
            } else {
                let _ = key.set_value(NAME, &tr);
            }
        }
        status()
    }

    pub fn disable() -> AutoStatus {
        let _ = ok(&["/Delete", "/TN", NAME, "/F"]);
        if let Ok((key, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN) {
            let _ = key.delete_value(NAME);
        }
        status()
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use std::path::PathBuf;

    fn plist_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join("Library/LaunchAgents/com.lsft32.ttaeng.plist")
    }

    fn esc(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    }

    pub fn status() -> AutoStatus {
        if plist_path().exists() { st(true, "launch_agent") } else { st(false, "none") }
    }

    pub fn enable(exe: &str) -> AutoStatus {
        let body = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>com.lsft32.ttaeng</string>
  <key>ProgramArguments</key><array><string>{}</string></array>
  <key>RunAtLoad</key><true/>
  <key>ProcessType</key><string>Interactive</string>
</dict>
</plist>
"#,
            esc(exe)
        );
        let p = plist_path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&p, body);
        status()
    }

    pub fn disable() -> AutoStatus {
        let _ = std::fs::remove_file(plist_path());
        status()
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use super::*;
    pub fn status() -> AutoStatus {
        st(false, "none")
    }
    pub fn enable(_exe: &str) -> AutoStatus {
        st(false, "none")
    }
    pub fn disable() -> AutoStatus {
        st(false, "none")
    }
}

pub use imp::{disable, enable, status};

/// 예전 방식(Run 키)으로 등록돼 있으면 작업 스케줄러로 옮긴다. Run 키는 다른 시작프로그램 뒤에 줄을 서서 늦게 실행된다.
pub fn migrate(exe: &str) {
    if status().method == "run_key" {
        enable(exe);
    }
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn task_xml_is_user_scoped_and_escaped() {
        let x = super::imp::task_xml(r"PC\me", r"C:\A&B\ttaeng.exe");
        assert!(x.contains(r"<LogonTrigger><Enabled>true</Enabled><UserId>PC\me</UserId>"));
        assert!(x.contains(r"<Command>C:\A&amp;B\ttaeng.exe</Command>"));
        assert!(x.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        assert!(x.contains("<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>"));
    }
}
