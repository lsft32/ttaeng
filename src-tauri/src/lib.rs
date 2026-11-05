mod advice;
mod autostart;
mod collector;
mod history;
mod judge;
mod model;
mod network;
mod platform;
mod probe;
mod session;
mod startup;
mod update;

use history::Store;
use model::{BootRecord, SessionView};
use serde::Serialize;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, State, WindowEvent, Wry};

pub struct Shared {
    pub store: Store,
    pub view: Mutex<Option<SessionView>>,
    pub demo: Mutex<Option<BootRecord>>,
    pub running: AtomicBool,
    pub auto_item: Mutex<Option<CheckMenuItem<Wry>>>,
    pub update: update::UpdateState,
    pub update_item: Mutex<Option<MenuItem<Wry>>>,
}

pub mod ui {
    use tauri::{AppHandle, Manager, PhysicalPosition};

    pub fn show_details(app: &AppHandle) {
        if let Some(w) = app.get_webview_window("details") {
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
    }

    /// 주 모니터 작업 영역(작업표시줄·Dock 제외)의 오른쪽 아래에 붙인다.
    pub fn show_overlay(app: &AppHandle) {
        let Some(w) = app.get_webview_window("overlay") else { return };
        if let (Ok(Some(m)), Ok(size)) = (w.primary_monitor(), w.outer_size()) {
            let wa = m.work_area();
            let margin = (16.0 * m.scale_factor()) as i32;
            let x = wa.position.x + wa.size.width as i32 - size.width as i32 - margin;
            let y = wa.position.y + wa.size.height as i32 - size.height as i32 - margin;
            let _ = w.set_position(PhysicalPosition::new(x, y));
        }
        let _ = w.show();
    }
}

fn self_exe() -> String {
    std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
}

#[derive(Serialize)]
struct HistoryPayload {
    records: Vec<BootRecord>,
    demo: Option<BootRecord>,
}

#[tauri::command]
fn get_view(state: State<Arc<Shared>>) -> Option<SessionView> {
    state.view.lock().ok().and_then(|v| v.clone())
}

#[tauri::command]
fn get_history(state: State<Arc<Shared>>) -> HistoryPayload {
    HistoryPayload {
        records: state.store.load_history(),
        demo: state.demo.lock().ok().and_then(|d| d.clone()),
    }
}

#[tauri::command]
fn get_autostart() -> autostart::AutoStatus {
    autostart::status()
}

#[tauri::command]
fn set_autostart(enabled: bool, state: State<Arc<Shared>>) -> autostart::AutoStatus {
    let st = if enabled { autostart::enable(&self_exe()) } else { autostart::disable() };
    if let Ok(g) = state.auto_item.lock() {
        if let Some(item) = g.as_ref() {
            let _ = item.set_checked(st.enabled);
        }
    }
    st
}

#[tauri::command]
fn open_details(app: AppHandle) {
    ui::show_details(&app);
}

#[tauri::command]
fn hide_overlay(app: AppHandle) {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.hide();
    }
}

#[tauri::command]
fn start_demo(app: AppHandle) {
    session::start(app, true);
}

#[tauri::command]
fn get_update(app: AppHandle) -> update::UpdateView {
    update::view(&app)
}

#[tauri::command]
fn check_update(app: AppHandle, force: bool) {
    update::check(&app, force);
}

#[tauri::command]
fn install_update(app: AppHandle) -> Result<(), String> {
    update::install(&app)
}

fn build_tray(app: &tauri::App, shared: &Arc<Shared>) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "열기", true, None::<&str>)?;
    let demo = MenuItem::with_id(app, "demo", "지금 측정해 보기", true, None::<&str>)?;
    let auto = CheckMenuItem::with_id(app, "auto", "로그인 시 자동 실행", true, autostart::status().enabled, None::<&str>)?;
    let upd = MenuItem::with_id(app, "update", "업데이트 확인", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &demo, &sep, &auto, &upd, &quit])?;
    if let Ok(mut g) = shared.auto_item.lock() {
        *g = Some(auto.clone());
    }
    if let Ok(mut g) = shared.update_item.lock() {
        *g = Some(upd.clone());
    }
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip(platform::APP_NAME)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, ev| match ev.id().as_ref() {
            "open" => ui::show_details(app),
            "demo" => session::start(app.clone(), true),
            "auto" => {
                let st = if autostart::status().enabled { autostart::disable() } else { autostart::enable(&self_exe()) };
                let _ = auto.set_checked(st.enabled);
            }
            "update" => {
                // 받아 둔 새 버전이 있으면 설치, 없으면 바로 확인한다. 결과는 상세 화면에 보인다.
                ui::show_details(app);
                if update::view(app).available.is_some() {
                    let _ = update::install(app);
                } else {
                    update::check(app, true);
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                ui::show_details(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| ui::show_details(app)))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let shared = Arc::new(Shared {
                store: Store::new(dir),
                view: Mutex::new(None),
                demo: Mutex::new(None),
                running: AtomicBool::new(false),
                auto_item: Mutex::new(None),
                update: update::UpdateState::default(),
                update_item: Mutex::new(None),
            });
            app.manage(shared.clone());

            // 개발 빌드는 실행 파일 위치가 임시라 자동 시작을 등록하지 않는다.
            let mut settings = shared.store.load_settings();
            if cfg!(not(debug_assertions)) {
                if !settings.autostart_initialized {
                    autostart::enable(&self_exe());
                    settings.autostart_initialized = true;
                    shared.store.save_settings(&settings);
                } else {
                    autostart::migrate(&self_exe());
                }
            }
            build_tray(app, &shared)?;
            // 동작 확인용: TTAENG_DEMO=1이면 부팅 측정 대신 데모 측정으로 시작한다.
            let demo = std::env::var("TTAENG_DEMO").is_ok_and(|v| v == "1");
            session::start(app.handle().clone(), demo);
            Ok(())
        })
        .on_window_event(|w, e| {
            if let WindowEvent::CloseRequested { api, .. } = e {
                api.prevent_close();
                let _ = w.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_view,
            get_history,
            get_autostart,
            set_autostart,
            open_details,
            hide_overlay,
            start_demo,
            get_update,
            check_update,
            install_update
        ])
        .build(tauri::generate_context!())
        .expect("땡해줘 실행 실패")
        .run(|_app, e| {
            if let RunEvent::ExitRequested { api, code, .. } = e {
                // 창을 모두 닫아도 트레이에 남는다. 트레이 "종료"는 code가 있다.
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
