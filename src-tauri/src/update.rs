//! 업데이트 확인·설치. 부팅 측정을 흔들지 않도록 측정 중에는 확인도 설치도 하지 않는다.
//! 설치는 항상 사용자가 누를 때만 한다(요구서 F11).

use crate::Shared;
use crate::model::now_f;
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// 자동 확인 간격. 수동 확인(트레이·버튼)은 이 제한을 받지 않는다.
pub const AUTO_INTERVAL_S: u64 = 6 * 3600;

#[derive(Debug, Clone, Serialize)]
pub struct Available {
    pub version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct UpdateView {
    pub current: String,
    pub available: Option<Available>,
    pub checking: bool,
    pub installing: bool,
    pub measuring: bool,
    pub error: Option<String>,
    pub last_check: u64,
}

#[derive(Default)]
pub struct UpdateState {
    view: Mutex<UpdateView>,
    pending: Mutex<Option<Update>>,
}

/// 자동 확인을 할 차례인지. 순수 함수라 테스트 대상.
pub fn due(now: u64, last: u64, force: bool) -> bool {
    force || last == 0 || now.saturating_sub(last) >= AUTO_INTERVAL_S
}

fn shared(app: &AppHandle) -> Arc<Shared> {
    app.state::<Arc<Shared>>().inner().clone()
}

pub fn view(app: &AppHandle) -> UpdateView {
    let sh = shared(app);
    let mut v = sh.update.view.lock().map(|v| v.clone()).unwrap_or_default();
    v.current = app.package_info().version.to_string();
    v.measuring = sh.running.load(Ordering::SeqCst);
    if v.last_check == 0 {
        v.last_check = sh.store.load_settings().last_update_check;
    }
    v
}

fn set(app: &AppHandle, f: impl FnOnce(&mut UpdateView)) {
    let sh = shared(app);
    if let Ok(mut v) = sh.update.view.lock() {
        f(&mut v);
    }
    let v = view(app);
    let label = match &v.available {
        Some(a) => format!("업데이트 설치 ({})", a.version),
        None if v.checking => "업데이트 확인 중…".to_string(),
        None => "업데이트 확인".to_string(),
    };
    // 트레이 갱신은 메인 스레드를 기다리므로 잠금 밖에서 한다.
    let item = sh.update_item.lock().ok().and_then(|g| g.clone());
    if let Some(item) = item {
        let _ = item.set_text(label);
    }
    let _ = app.emit("update-state", &v);
}

/// 측정 상태가 바뀌었을 때 화면·트레이를 다시 그리게 한다.
pub fn notify(app: &AppHandle) {
    set(app, |_| {});
}

/// 새 버전이 있는지 확인한다. 측정 중이면 하지 않는다.
pub fn check(app: &AppHandle, force: bool) {
    let sh = shared(app);
    if sh.running.load(Ordering::SeqCst) {
        return;
    }
    let now = now_f() as u64;
    let mut settings = sh.store.load_settings();
    if !due(now, settings.last_update_check, force) {
        return;
    }
    {
        let Ok(mut v) = sh.update.view.lock() else { return };
        if v.checking || v.installing {
            return;
        }
        v.checking = true;
    }
    settings.last_update_check = now;
    sh.store.save_settings(&settings);
    set(app, |v| v.error = None);

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = match app.updater() {
            Ok(u) => u.check().await.map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        };
        let sh = shared(&app);
        match result {
            Ok(Some(update)) => {
                let avail = Available { version: update.version.clone(), notes: update.body.clone(), date: update.date.map(|d| d.to_string()) };
                if let Ok(mut p) = sh.update.pending.lock() {
                    *p = Some(update);
                }
                set(&app, |v| {
                    v.checking = false;
                    v.available = Some(avail);
                    v.last_check = now;
                });
            }
            Ok(None) => {
                if let Ok(mut p) = sh.update.pending.lock() {
                    *p = None;
                }
                set(&app, |v| {
                    v.checking = false;
                    v.available = None;
                    v.last_check = now;
                });
            }
            // 자동 확인 실패(오프라인 등)는 조용히 넘기고, 사용자가 누른 확인만 오류를 보여준다.
            Err(e) => set(&app, |v| {
                v.checking = false;
                v.error = force.then_some(e);
                v.last_check = now;
            }),
        }
    });
}

/// 받아 둔 업데이트를 내려받아 설치하고 다시 실행한다.
/// Windows는 설치 프로그램(passive)을 띄우면서 앱이 종료되고, 설치가 끝나면 설치 프로그램이 다시 실행한다.
pub fn install(app: &AppHandle) -> Result<(), String> {
    let sh = shared(app);
    if sh.running.load(Ordering::SeqCst) {
        return Err("측정이 끝나면 설치할 수 있습니다.".into());
    }
    let update = sh.update.pending.lock().ok().and_then(|p| p.clone()).ok_or("설치할 업데이트가 없습니다.")?;
    {
        let mut v = sh.update.view.lock().map_err(|e| e.to_string())?;
        if v.installing {
            return Ok(());
        }
        v.installing = true;
    }
    set(app, |v| v.error = None);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match update.download_and_install(|_, _| {}, || {}).await {
            Ok(()) => app.restart(),
            Err(e) => set(&app, |v| {
                v.installing = false;
                v.error = Some(e.to_string());
            }),
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_check_interval() {
        assert!(due(1000, 0, false), "확인한 적 없으면 확인");
        assert!(!due(1000 + AUTO_INTERVAL_S - 1, 1000, false), "6시간 안이면 건너뜀");
        assert!(due(1000 + AUTO_INTERVAL_S, 1000, false));
        assert!(due(1001, 1000, true), "수동 확인은 항상");
        assert!(due(500, 1000, false) == false, "시계가 거꾸로 가도 연속 확인하지 않음");
    }
}
