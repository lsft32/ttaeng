//! 측정 세션: 1초마다 조회 → 판정 → 화면 이벤트. 끝나면 기록을 저장하고 조회를 멈춘다.

use crate::collector::Collector;
use crate::judge::{DetectRules, Judge, th};
use crate::model::*;
use crate::{Shared, advice, network, platform, startup, ui};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// 판정 후 백그라운드 작업을 더 관찰하는 시간.
const TAIL_S: f64 = 20.0;
/// 이 시간보다 늦게 실행됐으면 부팅 직후가 아니라고 본다.
const LATE_S: f64 = 600.0;

pub fn start(app: AppHandle, demo: bool) {
    let shared = app.state::<Arc<Shared>>().inner().clone();
    if shared.running.swap(true, Ordering::SeqCst) {
        if demo {
            ui::show_overlay(&app);
        }
        return;
    }
    thread::spawn(move || {
        run(&app, &shared, demo);
        shared.running.store(false, Ordering::SeqCst);
    });
}

fn find_anchor(procs: &ProcTable, boot: u64, uid: &Option<String>) -> Option<f64> {
    procs
        .values()
        .filter(|p| p.file_lc == platform::ANCHOR_FILE && p.start >= boot)
        .filter(|p| match (&p.uid, uid) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        })
        .map(|p| p.start)
        .min()
        .map(|s| s as f64)
}

fn background(procs: &ProcTable, judge: &Judge) -> Vec<BackgroundView> {
    let mut agg: BTreeMap<&str, f64> = BTreeMap::new();
    for p in procs.values() {
        if let Some((_, label)) = platform::BACKGROUND.iter().find(|(f, _)| *f == p.file_lc) {
            *agg.entry(label).or_default() += p.cpu as f64;
        }
    }
    let mut out: Vec<BackgroundView> = agg
        .into_iter()
        .filter(|(_, cpu)| *cpu >= 5.0)
        .map(|(name, cpu)| BackgroundView { name: name.to_string(), cpu: round1(cpu) })
        .collect();
    for v in judge.item_views().into_iter().filter(|v| v.state == ItemStateKind::Persistent) {
        out.push(BackgroundView { name: format!("{} (지속형)", v.name), cpu: v.cpu });
    }
    out.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
    out
}

fn boot_kind(anchor: f64, boot: u64, demo: bool) -> String {
    if demo {
        "demo".into()
    } else if cfg!(windows) && anchor - boot as f64 > LATE_S {
        "fast_startup".into()
    } else {
        "cold".into()
    }
}

#[allow(clippy::too_many_arguments)]
fn make_view(
    judge: &Judge,
    t: f64,
    demo: bool,
    phase: &str,
    net_s: Option<f64>,
    background: Vec<BackgroundView>,
    top_advice: Option<Advice>,
) -> SessionView {
    let checklist = judge.checklist(t);
    let network_note = match (net_s, checklist.network) {
        (Some(s), _) => format!("연결됨 ({s:.1}초)"),
        (None, Check::Skip) => "네트워크 없음".into(),
        _ => "확인 중".into(),
    };
    SessionView {
        demo,
        t: round1(t),
        phase: phase.into(),
        ready_s: judge.ready_s,
        forced: judge.forced,
        grade: judge.ready_s.map(|r| advice::grade(r, judge.forced)),
        checklist,
        network_note,
        progress: judge.progress(),
        items: judge.item_views(),
        background,
        top_advice,
    }
}

fn run(app: &AppHandle, shared: &Shared, demo: bool) {
    let Some(mut col) = Collector::new() else {
        ui::show_details(app);
        return;
    };
    thread::sleep(Duration::from_secs(1));
    let first = col.sample();
    let boot = Collector::boot_time();
    let self_uid = first.procs.get(&col.self_pid).and_then(|p| p.uid.clone());
    let now = now_f();
    let anchor = if demo { now } else { find_anchor(&first.procs, boot, &self_uid).unwrap_or(now) };
    if !demo && now - anchor > LATE_S {
        ui::show_details(app);
        return;
    }

    let self_exe = std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    let items = startup::collect(&shared.store, &self_exe);
    let history = shared.store.load_history();
    let skip_wait: HashSet<String> = history
        .last()
        .map(|r| r.items.iter().filter(|i| i.state == ItemStateKind::NotLaunched).map(|i| i.key.clone()).collect())
        .unwrap_or_default();
    let rules = DetectRules {
        shell_files: platform::SHELL_FILES.iter().map(|s| s.to_string()).collect(),
        system_prefixes: platform::system_prefixes(),
        self_pid: col.self_pid,
        self_uid,
    };
    let lookback = if demo { f64::INFINITY } else { th::ROOT_LOOKBACK };
    let mut judge = Judge::new(anchor, items, &skip_wait, rules, lookback);

    let net = Arc::new(Mutex::new(None));
    network::spawn(anchor, net.clone());
    if let Ok(mut d) = shared.demo.lock() {
        if demo {
            *d = None;
        }
    }
    ui::show_overlay(app);

    let mut self_cpu_sum = 0.0;
    let mut samples = 0u32;
    let mut bg_seen: BTreeSet<String> = BTreeSet::new();
    let mut next = Instant::now();
    loop {
        next += Duration::from_secs(1);
        if let Some(wait) = next.checked_duration_since(Instant::now()) {
            thread::sleep(wait);
        }
        let s = col.sample();
        let t = now_f() - anchor;
        let net_s = net.lock().ok().and_then(|g| *g);
        judge.tick(t, &s.procs, net_s, s.sys_cpu);
        self_cpu_sum += s.self_cpu;
        samples += 1;
        let bg = background(&s.procs, &judge);
        bg_seen.extend(bg.iter().map(|b| b.name.clone()));

        let finished = judge.ready_declared_t.is_some_and(|d| t >= d + TAIL_S) || t >= th::HARD_CAP + TAIL_S;
        if !finished {
            let phase = if judge.ready_s.is_some() { "ready" } else { "preparing" };
            publish(app, shared, make_view(&judge, t, demo, phase, net_s, bg, None));
            continue;
        }

        let ready_s = judge.ready_s.unwrap_or(round1(t));
        let mut rec = BootRecord {
            v: 1,
            os: platform::OS.into(),
            boot_kind: boot_kind(anchor, boot, demo),
            boot_time: boot,
            anchor_time: anchor as u64,
            power_to_desktop_s: if demo { 0.0 } else { round1(anchor - boot as f64) },
            event100: None,
            ready_s,
            forced: judge.forced,
            grade: advice::grade(ready_s, judge.forced),
            network_s: net_s,
            aux_idle_s: judge.aux_s,
            self_cpu_avg: round1(self_cpu_sum / samples.max(1) as f64),
            bottleneck: judge.bottleneck(),
            items: judge.item_records(),
            background: bg_seen.into_iter().collect(),
            advice: vec![],
        };
        #[cfg(windows)]
        if !demo {
            rec.event100 = startup::windows::read_event100(boot);
        }
        let prev: Vec<BootRecord> = history.into_iter().filter(|r| r.boot_kind != "demo").collect();
        rec.advice = advice::build(&rec, &prev);
        if demo {
            if let Ok(mut d) = shared.demo.lock() {
                *d = Some(rec.clone());
            }
        } else {
            let _ = shared.store.append_history(&rec);
        }
        let _ = app.emit("history-updated", ());
        publish(app, shared, make_view(&judge, t, demo, "done", net_s, bg, rec.advice.first().cloned()));
        startup::refresh_cache(&shared.store);
        return;
    }
}

/// 쓰면 안 되는 동안은 "얼음!", 사용 가능 판정 후에는 "땡!".
pub fn freeze_label(ready: bool) -> &'static str {
    if ready { "땡!" } else { "얼음!" }
}

fn publish(app: &AppHandle, shared: &Shared, view: SessionView) {
    let _ = app.emit("session", &view);
    let ready = view.ready_s.is_some();
    let changed = match shared.view.lock() {
        Ok(mut v) => {
            let prev = v.as_ref().map(|p| p.ready_s.is_some());
            *v = Some(view);
            prev != Some(ready)
        }
        Err(_) => false,
    };
    // 트레이 갱신은 메인 스레드를 기다리므로 잠금을 놓은 뒤에 한다.
    if changed {
        if let Some(tray) = app.tray_by_id("main") {
            let _ = tray.set_tooltip(Some(format!("{} · {}", platform::APP_NAME, freeze_label(ready))));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::freeze_label;

    #[test]
    fn freeze_label_follows_ready() {
        assert_eq!(freeze_label(false), "얼음!");
        assert_eq!(freeze_label(true), "땡!");
    }
}
