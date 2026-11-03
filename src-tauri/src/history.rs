//! 이력·설정·캐시 파일. 형식은 docs/03-data-schema.md.

use crate::model::BootRecord;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const KEEP: usize = 200;

pub struct Store {
    dir: PathBuf,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub autostart_initialized: bool,
    /// 마지막 업데이트 확인 시각(epoch 초). 0이면 확인한 적 없음.
    #[serde(default)]
    pub last_update_check: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogonTask {
    pub name: String,
    pub exe: String,
    /// 작업 경로(폴더 + 이름). 측정 시작 때 작업 파일이 아직 있는지 확인하는 데 쓴다. v0.3.4
    #[serde(default)]
    pub path: String,
    /// 로그온 트리거 지연(초)
    #[serde(default)]
    pub delay_s: f64,
    /// 유휴 상태일 때만 실행
    #[serde(default)]
    pub idle_only: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct LogonTaskCache {
    pub updated: u64,
    pub tasks: Vec<LogonTask>,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&dir);
        Self { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    pub fn load_history(&self) -> Vec<BootRecord> {
        let Ok(text) = fs::read_to_string(self.path("history.jsonl")) else { return vec![] };
        text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
    }

    pub fn append_history(&self, rec: &BootRecord) -> std::io::Result<()> {
        let mut all = self.load_history();
        all.push(rec.clone());
        if all.len() > KEEP {
            all.drain(0..all.len() - KEEP);
        }
        let mut buf = String::new();
        for r in &all {
            buf.push_str(&serde_json::to_string(r).unwrap_or_default());
            buf.push('\n');
        }
        write_atomic(&self.path("history.jsonl"), buf.as_bytes())
    }

    pub fn load_settings(&self) -> Settings {
        read_json(&self.path("settings.json")).unwrap_or_default()
    }

    pub fn save_settings(&self, s: &Settings) {
        let _ = write_atomic(&self.path("settings.json"), serde_json::to_string_pretty(s).unwrap_or_default().as_bytes());
    }

    pub fn load_tasks(&self) -> LogonTaskCache {
        read_json(&self.path("logon_tasks.json")).unwrap_or_default()
    }

    pub fn save_tasks(&self, c: &LogonTaskCache) {
        let _ = write_atomic(&self.path("logon_tasks.json"), serde_json::to_string_pretty(c).unwrap_or_default().as_bytes());
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(p: &Path) -> Option<T> {
    serde_json::from_str(&fs::read_to_string(p).ok()?).ok()
}

/// 전원이 갑자기 꺼져도 기존 파일이 깨지지 않도록 임시 파일에 쓴 뒤 교체한다.
fn write_atomic(p: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = p.with_extension("tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, p)
}
