//! 로그인 자동 시작 등록.
//! Windows는 Run 키가 탐색기 시작 후 지연 실행되므로 작업 스케줄러 "로그온 시"를 먼저 시도한다.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AutoStatus {
    pub enabled: bool,
    /// `task_scheduler` / `run_key` / `launch_agent` / `none`
    pub method: String,
}

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

    pub fn enable(exe: &str) -> AutoStatus {
        let tr = format!("\"{exe}\"");
        let task = ok(&["/Create", "/TN", NAME, "/TR", &tr, "/SC", "ONLOGON", "/RL", "LIMITED", "/F"]);
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
