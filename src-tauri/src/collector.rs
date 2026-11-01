//! sysinfo로 1초마다 프로세스 표를 만든다.

use crate::model::{ProcInfo, ProcTable};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

pub struct Collector {
    sys: System,
    pub self_pid: u32,
}

pub struct Sample {
    pub procs: ProcTable,
    /// 시스템 전체 CPU 사용률(0~100).
    pub sys_cpu: f64,
    /// 앱 자신의 CPU(코어 1개 기준 %).
    pub self_cpu: f64,
}

fn refresh_kind() -> ProcessRefreshKind {
    ProcessRefreshKind::nothing()
        .with_cpu()
        .with_disk_usage()
        .with_exe(UpdateKind::OnlyIfNotSet)
        .with_user(UpdateKind::OnlyIfNotSet)
}

pub fn normalize_exe(path: &str) -> String {
    path.trim_start_matches("\\\\?\\").to_lowercase()
}

impl Collector {
    pub fn new() -> Option<Self> {
        let mut sys = System::new().ok()?;
        sys.refresh_cpu_usage();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind());
        Some(Self { sys, self_pid: std::process::id() })
    }

    pub fn boot_time() -> u64 {
        System::boot_time().unwrap_or(0)
    }

    pub fn sample(&mut self) -> Sample {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh_kind());
        let mut self_cpu = 0.0;
        let procs: ProcTable = self
            .sys
            .processes()
            .iter()
            .map(|(pid, p)| {
                let exe_lc = p.exe().map(|e| normalize_exe(&e.to_string_lossy()));
                let name = p.name().to_string_lossy().to_string();
                let file_lc = p
                    .exe()
                    .and_then(|e| e.file_name())
                    .map(|f| f.to_string_lossy().to_lowercase())
                    .unwrap_or_else(|| name.to_lowercase());
                let du = p.disk_usage();
                let info = ProcInfo {
                    pid: pid.as_u32(),
                    ppid: p.parent().map(|pp| pp.as_u32()),
                    exe_lc,
                    file_lc,
                    name,
                    start: p.start_time(),
                    cpu: p.cpu_usage(),
                    disk: du.read_bytes + du.written_bytes,
                    uid: p.user_id().map(|u| format!("{u:?}")),
                };
                if info.pid == self.self_pid {
                    self_cpu = info.cpu as f64;
                }
                (info.pid, info)
            })
            .collect();
        Sample { procs, sys_cpu: self.sys.global_cpu_usage() as f64, self_cpu }
    }
}
