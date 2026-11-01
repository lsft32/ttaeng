//! macOS 시작 항목: RunAtLoad LaunchAgents. 로그인 항목·윈도우 복원 앱은 판정 엔진의 자동 감지로 잡는다.

use super::matcher_for;
use crate::model::{Source, StartupItem};
use std::path::PathBuf;

pub fn collect() -> Vec<StartupItem> {
    let mut dirs = vec![PathBuf::from("/Library/LaunchAgents")];
    if let Ok(home) = std::env::var("HOME") {
        dirs.insert(0, PathBuf::from(home).join("Library/LaunchAgents"));
    }
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("plist") {
                continue;
            }
            let Ok(v) = plist::Value::from_file(&path) else { continue };
            let Some(d) = v.as_dictionary() else { continue };
            if d.get("RunAtLoad").and_then(|x| x.as_boolean()) != Some(true) {
                continue;
            }
            if d.get("Disabled").and_then(|x| x.as_boolean()) == Some(true) {
                continue;
            }
            let exe = d
                .get("Program")
                .and_then(|x| x.as_string())
                .or_else(|| d.get("ProgramArguments").and_then(|x| x.as_array()).and_then(|a| a.first()).and_then(|x| x.as_string()))
                .unwrap_or("")
                .to_string();
            let name = d
                .get("Label")
                .and_then(|x| x.as_string())
                .map(String::from)
                .unwrap_or_else(|| path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
            out.push(StartupItem::new(&name, Source::LaunchAgent, matcher_for(&exe)));
        }
    }
    out
}
