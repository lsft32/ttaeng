use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 1초 조회 시점의 프로세스 한 개.
#[derive(Debug, Clone, Default)]
pub struct ProcInfo {
    pub pid: u32,
    pub ppid: Option<u32>,
    /// 실행 파일 전체 경로(소문자). 권한 부족 등으로 못 읽으면 None.
    pub exe_lc: Option<String>,
    /// 실행 파일 이름(소문자). 경로를 못 읽어도 프로세스 이름으로 채운다.
    pub file_lc: String,
    /// 표시용 프로세스 이름(원래 대소문자).
    pub name: String,
    pub start: u64,
    /// 코어 1개 기준 %.
    pub cpu: f32,
    /// 직전 조회 이후 읽기+쓰기 바이트.
    pub disk: u64,
    pub uid: Option<String>,
}

pub type ProcTable = HashMap<u32, ProcInfo>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    RunKey,
    StartupFolder,
    LogonTask,
    LaunchAgent,
    Detected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Matcher {
    Path(String),
    FileName(String),
    None,
}

#[derive(Debug, Clone)]
pub struct StartupItem {
    pub key: String,
    pub name: String,
    pub source: Source,
    pub matcher: Matcher,
}

impl StartupItem {
    pub fn new(name: &str, source: Source, matcher: Matcher) -> Self {
        let key = match &matcher {
            Matcher::Path(p) => p.clone(),
            Matcher::FileName(f) => f.clone(),
            Matcher::None => format!("name:{}", name.to_lowercase()),
        };
        Self { key, name: name.to_string(), source, matcher }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStateKind {
    Waiting,
    /// 45초 안에 실행되지 않음. 판정을 막지 않고 계속 지켜본다(요구서 v1.5).
    Deferred,
    Running,
    Settled,
    Exited,
    Persistent,
    NotLaunched,
    Untrackable,
}

impl ItemStateKind {
    /// 판정을 더 이상 막지 않는 상태인지.
    pub fn is_done(self) -> bool {
        !matches!(self, Self::Waiting | Self::Deferred | Self::Running)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Advice {
    pub rule: String,
    pub title: String,
    pub detail: String,
    pub item: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemRecord {
    pub key: String,
    pub name: String,
    pub source: Source,
    pub state: ItemStateKind,
    pub launch_s: Option<f64>,
    pub settle_s: Option<f64>,
    pub peak_cpu: f64,
    pub cpu_seconds: f64,
    pub disk_mb: f64,
    /// 45초 이후 실행(늦은 실행). v1.5
    #[serde(default)]
    pub late: bool,
}

/// 땡! 이후 늦게 실행된 항목 때문에 다시 얼음!이 된 구간.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Refreeze {
    pub key: String,
    pub name: String,
    pub from_s: f64,
    pub to_s: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event100 {
    pub main_path_ms: u64,
    pub post_boot_ms: u64,
    pub boot_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BootRecord {
    pub v: u32,
    pub os: String,
    pub boot_kind: String,
    pub boot_time: u64,
    pub anchor_time: u64,
    pub power_to_desktop_s: f64,
    pub event100: Option<Event100>,
    pub ready_s: f64,
    pub forced: bool,
    pub grade: String,
    pub network_s: Option<f64>,
    pub aux_idle_s: Option<f64>,
    pub self_cpu_avg: f64,
    pub bottleneck: Option<String>,
    pub items: Vec<ItemRecord>,
    pub background: Vec<String>,
    pub advice: Vec<Advice>,
    // v1.4 추가 필드. 이전 기록에는 없으므로 기본값으로 읽는다.
    #[serde(default)]
    pub boot_source: String,
    #[serde(default)]
    pub power_to_logon_s: Option<f64>,
    #[serde(default)]
    pub logon_to_desktop_s: Option<f64>,
    #[serde(default)]
    pub measure_start_s: f64,
    #[serde(default)]
    pub late_start: bool,
    #[serde(default)]
    pub network_before_start: bool,
    // v1.5
    #[serde(default)]
    pub observe_until_s: f64,
    #[serde(default)]
    pub all_launched_s: Option<f64>,
    #[serde(default)]
    pub refreeze: Vec<Refreeze>,
}

/// 기록 스키마 버전. 2부터 늦은 실행을 10분까지 지켜본 기록이다.
pub const RECORD_V: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Check {
    Ok,
    Wait,
    Skip,
}

#[derive(Debug, Clone, Serialize)]
pub struct Checklist {
    pub desktop: Check,
    pub items: Check,
    pub network: Check,
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemView {
    pub key: String,
    pub name: String,
    pub source: Source,
    pub state: ItemStateKind,
    pub cpu: f64,
    pub launch_s: Option<f64>,
    pub settle_s: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackgroundView {
    pub name: String,
    pub cpu: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionView {
    pub demo: bool,
    pub t: f64,
    pub phase: String,
    pub ready_s: Option<f64>,
    pub forced: bool,
    pub grade: Option<String>,
    pub checklist: Checklist,
    pub network_note: String,
    pub progress: Progress,
    pub items: Vec<ItemView>,
    pub background: Vec<BackgroundView>,
    pub top_advice: Option<Advice>,
    /// 측정을 시작한 시각(기준 대비 초). 30초 이상이면 화면에 참고용임을 알린다.
    pub measure_start_s: f64,
    /// 땡! 이후 늦은 항목 때문에 다시 바쁜 상태.
    pub refrozen: bool,
    /// 아직 실행되지 않고 기다리는 항목 수.
    pub deferred: usize,
}

pub fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

pub fn now_f() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
