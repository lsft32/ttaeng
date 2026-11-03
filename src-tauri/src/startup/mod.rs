//! 시작 항목 수집. OS별 등록 위치를 읽고, 중복과 앱 자신을 뺀다.

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
        .filter(|i| i.key != self_key && i.name != crate::platform::APP_NAME)
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
    let matcher = matcher_for(exe);
    // 경로가 있는 항목만 확인한다(파일 이름만 있으면 PATH 검색이라 알 수 없음).
    let missing = matches!(matcher, Matcher::Path(_)) && !Path::new(exe.trim().trim_matches('"')).exists();
    let mut it = StartupItem::new(name, source, matcher);
    it.missing = missing;
    it
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_are_untrackable() {
        // 경로 구분자가 OS마다 달라 각 OS의 경로로 확인한다.
        #[cfg(not(windows))]
        assert_eq!(matcher_for("/bin/sh"), Matcher::None);
        #[cfg(windows)]
        assert_eq!(matcher_for("C:\\Windows\\System32\\rundll32.exe"), Matcher::None);
        assert_eq!(matcher_for(""), Matcher::None);
    }

    #[test]
    fn missing_executable_is_flagged() {
        let gone = item("Gone", Source::RunKey, "C:/no/such/dir/gone.exe");
        assert!(gone.missing);
        let bare = item("Bare", Source::RunKey, "LGLivelyWallpaper.exe");
        assert!(!bare.missing, "파일 이름만 있으면 판단하지 않음");
    }

    #[test]
    fn bare_file_name() {
        assert_eq!(matcher_for("LGLivelyWallpaper.exe"), Matcher::FileName("lglivelywallpaper.exe".into()));
    }
}
