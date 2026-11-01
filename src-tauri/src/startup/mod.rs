//! 시작 항목 수집. OS별 등록 위치를 읽고, 중복과 Settle 자신을 뺀다.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
pub mod windows;

use crate::history::Store;
use crate::model::{Matcher, Source, StartupItem};
use std::collections::HashSet;
use std::path::Path;

/// 다른 프로그램을 대신 실행하는 호스트. 어떤 항목인지 실행 파일로 구분할 수 없다.
const HOSTS: &[&str] = &[
    "cmd.exe", "wscript.exe", "cscript.exe", "rundll32.exe", "powershell.exe", "pwsh.exe", "mshta.exe",
    "conhost.exe", "explorer.exe", "sh", "bash", "zsh", "osascript", "open", "python", "python3", "env",
];

pub fn normalize_path(p: &str) -> String {
    let path = Path::new(p);
    let resolved = std::fs::canonicalize(path).map(|c| c.to_string_lossy().to_string()).unwrap_or_else(|_| p.to_string());
    crate::collector::normalize_exe(&resolved)
}

/// 실행 파일 문자열(경로 또는 파일명)로 매칭 방식을 정한다.
pub fn matcher_for(exe: &str) -> Matcher {
    let exe = exe.trim().trim_matches('"');
    if exe.is_empty() {
        return Matcher::None;
    }
    let file = Path::new(exe).file_name().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default();
    if file.is_empty() || HOSTS.contains(&file.as_str()) {
        return Matcher::None;
    }
    if cfg!(windows) && !file.ends_with(".exe") {
        return Matcher::None;
    }
    if exe.contains('\\') || exe.contains('/') {
        Matcher::Path(normalize_path(exe))
    } else {
        Matcher::FileName(file)
    }
}

pub fn collect(store: &Store, self_exe: &str) -> Vec<StartupItem> {
    #[cfg(windows)]
    let raw = windows::collect(store);
    #[cfg(target_os = "macos")]
    let raw = {
        let _ = store;
        macos::collect()
    };
    #[cfg(not(any(windows, target_os = "macos")))]
    let raw: Vec<StartupItem> = {
        let _ = store;
        vec![]
    };

    let self_key = normalize_path(self_exe);
    let mut seen = HashSet::new();
    raw.into_iter()
        .filter(|i| i.key != self_key && i.name != "Settle")
        .filter(|i| seen.insert(i.key.clone()))
        .collect()
}

/// 부팅 중에 하기엔 무거운 목록 조회를 세션 종료 후 미리 해 둔다.
pub fn refresh_cache(store: &Store) {
    #[cfg(windows)]
    windows::refresh_logon_tasks(store);
    #[cfg(not(windows))]
    let _ = store;
}

#[allow(dead_code)]
pub fn item(name: &str, source: Source, exe: &str) -> StartupItem {
    StartupItem::new(name, source, matcher_for(exe))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_are_untrackable() {
        assert_eq!(matcher_for("C:\\Windows\\System32\\rundll32.exe"), Matcher::None);
        assert_eq!(matcher_for(""), Matcher::None);
    }

    #[test]
    fn bare_file_name() {
        assert_eq!(matcher_for("LGLivelyWallpaper.exe"), Matcher::FileName("lglivelywallpaper.exe".into()));
    }
}
