//! Windows 시작 항목: Run 키, 시작 폴더, 로그온 예약 작업(캐시).

use super::{item, matcher_for};
use crate::history::{LogonTask, LogonTaskCache, Store};
use crate::model::{Event100, Source, StartupItem};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use winreg::RegKey;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_WOW: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

pub fn hidden(cmd: &str) -> Command {
    let mut c = Command::new(cmd);
    c.creation_flags(CREATE_NO_WINDOW);
    c
}

pub fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('%') {
            Some(j) => {
                let var = &after[..j];
                match std::env::var(var) {
                    Ok(v) if !var.is_empty() => out.push_str(&v),
                    _ => {
                        out.push('%');
                        out.push_str(var);
                        out.push('%');
                    }
                }
                rest = &after[j + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// 명령줄에서 실행 파일 부분만 꺼낸다.
pub fn exe_from_command(cmd: &str) -> String {
    let s = expand_env(cmd.trim());
    if let Some(stripped) = s.strip_prefix('"') {
        return stripped.split('"').next().unwrap_or("").to_string();
    }
    let lower = s.to_lowercase();
    match lower.find(".exe") {
        Some(i) => s[..i + 4].to_string(),
        None => s.split_whitespace().next().unwrap_or("").to_string(),
    }
}

/// StartupApproved 값의 첫 바이트가 홀수면 사용자가 꺼 둔 항목이다(커뮤니티 확인 규칙, 공식 문서 없음).
fn disabled(sub: &str, name: &str) -> bool {
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE].iter().any(|h| {
        RegKey::predef(*h)
            .open_subkey(format!(r"{APPROVED}\{sub}"))
            .and_then(|k| k.get_raw_value(name))
            .map(|v| v.bytes.first().is_some_and(|b| b & 1 == 1))
            .unwrap_or(false)
    })
}

fn run_keys(out: &mut Vec<StartupItem>) {
    let keys = [(HKEY_CURRENT_USER, RUN, "Run"), (HKEY_LOCAL_MACHINE, RUN, "Run"), (HKEY_LOCAL_MACHINE, RUN_WOW, "Run32")];
    for (hive, path, sub) in keys {
        let Ok(key) = RegKey::predef(hive).open_subkey(path) else { continue };
        for (name, _) in key.enum_values().flatten() {
            if disabled(sub, &name) {
                continue;
            }
            let Ok(cmd) = key.get_value::<String, _>(&name) else { continue };
            out.push(item(&name, Source::RunKey, &exe_from_command(&cmd)));
        }
    }
}

fn startup_folders(out: &mut Vec<StartupItem>) {
    let mut dirs = Vec::new();
    if let Ok(a) = std::env::var("APPDATA") {
        dirs.push(PathBuf::from(a).join(r"Microsoft\Windows\Start Menu\Programs\Startup"));
    }
    if let Ok(p) = std::env::var("ProgramData") {
        dirs.push(PathBuf::from(p).join(r"Microsoft\Windows\Start Menu\Programs\StartUp"));
    }
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            let file = e.file_name().to_string_lossy().to_string();
            if file.eq_ignore_ascii_case("desktop.ini") || disabled("StartupFolder", &file) {
                continue;
            }
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(file.clone());
            let ext = path.extension().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
            let exe = match ext.as_str() {
                "lnk" => std::fs::read(&path).ok().and_then(|d| lnk_target(&d)).unwrap_or_default(),
                "exe" => path.to_string_lossy().to_string(),
                _ => String::new(),
            };
            out.push(StartupItem::new(&stem, Source::StartupFolder, matcher_for(&exe)));
        }
    }
}

fn u32le(d: &[u8], o: usize) -> Option<u32> {
    d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn u16le(d: &[u8], o: usize) -> Option<u16> {
    d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn cstr(d: &[u8], o: usize) -> Option<String> {
    let bytes = d.get(o..)?;
    let end = bytes.iter().position(|&b| b == 0)?;
    Some(String::from_utf8_lossy(&bytes[..end]).to_string())
}

fn wstr(d: &[u8], o: usize) -> Option<String> {
    let mut units = Vec::new();
    let mut i = o;
    loop {
        let u = u16le(d, i)?;
        if u == 0 {
            break;
        }
        units.push(u);
        i += 2;
    }
    Some(String::from_utf16_lossy(&units))
}

/// .lnk(MS-SHLLINK)의 LinkInfo에서 대상 경로를 읽는다.
pub fn lnk_target(d: &[u8]) -> Option<String> {
    if u32le(d, 0)? != 0x4C {
        return None;
    }
    let flags = u32le(d, 0x14)?;
    let mut off = 0x4C;
    if flags & 0x1 != 0 {
        off += 2 + u16le(d, off)? as usize;
    }
    if flags & 0x2 == 0 {
        return None;
    }
    let li = off;
    let header = u32le(d, li + 4)?;
    let li_flags = u32le(d, li + 8)?;
    if li_flags & 0x1 == 0 {
        return None;
    }
    let path = if header >= 0x24 {
        let base = wstr(d, li + u32le(d, li + 28)? as usize)?;
        let suffix = wstr(d, li + u32le(d, li + 32)? as usize).unwrap_or_default();
        base + &suffix
    } else {
        let base = cstr(d, li + u32le(d, li + 16)? as usize)?;
        let suffix = cstr(d, li + u32le(d, li + 24)? as usize).unwrap_or_default();
        base + &suffix
    };
    // ANSI 경로에 한글 등이 있으면 깨지므로 파일명만으로 매칭한다.
    if path.contains('\u{FFFD}') {
        let file = path.rsplit('\\').next().unwrap_or("");
        return (!file.contains('\u{FFFD}')).then(|| file.to_string());
    }
    Some(path)
}

fn logon_tasks(store: &Store, out: &mut Vec<StartupItem>) {
    for t in store.load_tasks().tasks {
        out.push(item(&t.name, Source::LogonTask, &exe_from_command(&t.exe)));
    }
}

pub fn collect(store: &Store) -> Vec<StartupItem> {
    let mut out = Vec::new();
    run_keys(&mut out);
    startup_folders(&mut out);
    logon_tasks(store, &mut out);
    out
}

const TASK_SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$me1 = "$env:USERDOMAIN\$env:USERNAME"; $me2 = $env:USERNAME
@(Get-ScheduledTask | Where-Object { $_.State -ne 'Disabled' -and $_.TaskPath -notlike '\Microsoft\Windows\*' } | ForEach-Object {
  $t = $_
  $lt = @($t.Triggers | Where-Object { $_.CimClass.CimClassName -eq 'MSFT_TaskLogonTrigger' -and $_.Enabled -ne $false -and (-not $_.UserId -or $_.UserId -eq $me1 -or $_.UserId -eq $me2) })
  $a = @($t.Actions | Where-Object { $_.Execute }) | Select-Object -First 1
  if ($lt.Count -gt 0 -and $a) { [pscustomobject]@{ name = $t.TaskName; exe = $a.Execute } }
}) | ConvertTo-Json -Compress
"#;

pub fn refresh_logon_tasks(store: &Store) {
    let Ok(out) = hidden("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", TASK_SCRIPT])
        .output()
    else {
        return;
    };
    if !out.status.success() {
        return;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let value: serde_json::Value = serde_json::from_str(text.trim()).unwrap_or(serde_json::Value::Array(vec![]));
    let list = match value {
        serde_json::Value::Array(a) => a,
        v @ serde_json::Value::Object(_) => vec![v],
        _ => vec![],
    };
    let sysroot = crate::platform::system_prefixes();
    let tasks: Vec<LogonTask> = list
        .into_iter()
        .filter_map(|v| serde_json::from_value::<LogonTask>(v).ok())
        .filter(|t| {
            let exe = expand_env(&t.exe).to_lowercase();
            !sysroot.iter().any(|s| exe.starts_with(s.as_str())) && t.name != crate::platform::APP_NAME
        })
        .collect();
    store.save_tasks(&LogonTaskCache { updated: crate::model::now_f() as u64, tasks });
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn iso_to_epoch(s: &str) -> Option<u64> {
    let n = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    let days = days_from_civil(n(0, 4)?, n(5, 7)?, n(8, 10)?);
    let secs = days * 86_400 + n(11, 13)? * 3600 + n(14, 16)? * 60 + n(17, 19)?;
    u64::try_from(secs).ok()
}

fn xml_data(xml: &str, name: &str) -> Option<String> {
    for q in ['\'', '"'] {
        let tag = format!("Name={q}{name}{q}>");
        if let Some(i) = xml.find(&tag) {
            let rest = &xml[i + tag.len()..];
            return rest.split('<').next().map(|s| s.to_string());
        }
    }
    None
}

/// 부팅 성능 이벤트(ID 100). 일반 권한으로는 보통 읽을 수 없다.
pub fn read_event100(boot_time: u64) -> Option<Event100> {
    let out = hidden("wevtutil")
        .args([
            "qe",
            "Microsoft-Windows-Diagnostics-Performance/Operational",
            "/q:*[System[EventID=100]]",
            "/c:1",
            "/rd:true",
            "/f:xml",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let xml = String::from_utf8_lossy(&out.stdout);
    let start = iso_to_epoch(&xml_data(&xml, "BootStartTime")?)?;
    if start.abs_diff(boot_time) > 300 {
        return None;
    }
    Some(Event100 {
        main_path_ms: xml_data(&xml, "MainPathBootTime")?.parse().ok()?,
        post_boot_ms: xml_data(&xml, "BootPostBootTime")?.parse().ok()?,
        boot_ms: xml_data(&xml, "BootTime")?.parse().ok()?,
    })
}

#[allow(dead_code)]
pub fn exists(p: &str) -> bool {
    Path::new(p).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_parsing() {
        assert_eq!(exe_from_command(r#""C:\Program Files\A\a.exe" --min"#), r"C:\Program Files\A\a.exe");
        assert_eq!(exe_from_command(r"C:\Tools\b.EXE /tray"), r"C:\Tools\b.EXE");
        assert_eq!(exe_from_command("LGLivelyWallpaper.exe"), "LGLivelyWallpaper.exe");
    }

    #[test]
    fn iso_epoch() {
        assert_eq!(iso_to_epoch("1970-01-02T00:00:00.0000000Z"), Some(86_400));
        assert_eq!(iso_to_epoch("2026-11-01T00:00:00Z"), Some(1_793_491_200));
    }

    #[test]
    fn real_startup_items_parse() {
        // 이 PC의 실제 시작 항목을 읽어 형식 오류가 없는지만 확인한다.
        let mut out = Vec::new();
        run_keys(&mut out);
        startup_folders(&mut out);
        for i in &out {
            println!("{:?} {} -> {:?}", i.source, i.name, i.matcher);
        }
    }
}
