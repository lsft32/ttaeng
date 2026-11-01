//! OS별 상수.

#[cfg(windows)]
mod imp {
    pub const OS: &str = "windows";
    /// 바탕화면 표시 시각(기준 시각)으로 쓰는 프로세스.
    pub const ANCHOR_FILE: &str = "explorer.exe";
    /// 시작 항목을 띄우는 셸. 자동 감지에서 부모로 인정한다.
    pub const SHELL_FILES: &[&str] = &["explorer.exe"];
    pub const NET_PROBE: &str = "www.msftconnecttest.com:80";
    pub const BACKGROUND: &[(&str, &str)] = &[
        ("msmpeng.exe", "Defender 검사"),
        ("mpdefendercoreservice.exe", "Defender 검사"),
        ("tiworker.exe", "Windows 업데이트 설치"),
        ("trustedinstaller.exe", "Windows 모듈 설치"),
        ("mousocoreworker.exe", "Windows 업데이트"),
        ("wuauclt.exe", "Windows 업데이트"),
        ("searchindexer.exe", "검색 인덱서"),
        ("searchprotocolhost.exe", "검색 인덱서"),
        ("onedrive.exe", "OneDrive 동기화"),
        ("compattelrunner.exe", "호환성 원격 분석"),
        ("mscorsvw.exe", ".NET 최적화"),
        ("sihclient.exe", "Windows 업데이트 상태 점검"),
    ];

    pub fn system_prefixes() -> Vec<String> {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        vec![format!("{}\\", root.trim_end_matches('\\').to_lowercase())]
    }
}

#[cfg(target_os = "macos")]
mod imp {
    pub const OS: &str = "macos";
    pub const ANCHOR_FILE: &str = "dock";
    pub const SHELL_FILES: &[&str] = &["launchd"];
    pub const NET_PROBE: &str = "captive.apple.com:80";
    pub const BACKGROUND: &[(&str, &str)] = &[
        ("mds", "Spotlight 인덱싱"),
        ("mds_stores", "Spotlight 인덱싱"),
        ("mdworker_shared", "Spotlight 인덱싱"),
        ("backupd", "Time Machine"),
        ("bird", "iCloud 동기화"),
        ("cloudd", "iCloud 동기화"),
        ("fileproviderd", "iCloud 동기화"),
        ("photoanalysisd", "사진 분석"),
        ("mediaanalysisd", "미디어 분석"),
        ("softwareupdated", "소프트웨어 업데이트"),
        ("xprotect", "XProtect 검사"),
        ("xprotectservice", "XProtect 검사"),
    ];

    pub fn system_prefixes() -> Vec<String> {
        ["/system/", "/usr/", "/bin/", "/sbin/", "/library/apple/"].iter().map(|s| s.to_string()).collect()
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    pub const OS: &str = "other";
    pub const ANCHOR_FILE: &str = "";
    pub const SHELL_FILES: &[&str] = &[];
    pub const NET_PROBE: &str = "example.com:80";
    pub const BACKGROUND: &[(&str, &str)] = &[];
    pub fn system_prefixes() -> Vec<String> {
        vec!["/usr/".into(), "/bin/".into(), "/sbin/".into()]
    }
}

pub use imp::*;

/// 화면·트레이·자동 시작 등록에 쓰는 앱 이름.
pub const APP_NAME: &str = "땡해줘";
