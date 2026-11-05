//! 측정 세션: 1초마다 조회 → 판정 → 화면 이벤트. 끝나면 기록을 저장하고 조회를 멈춘다.

use crate::collector::Collector;
use crate::judge::{DetectRules, Judge, th};
use crate::model::*;
use crate::{Shared, advice, network, platform, probe, startup, ui};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// 판정 후 백그라운드 작업을 더 관찰하는 시간.
const TAIL_S: f64 = 20.0;
/// 이 시간보다 늦게 실행됐으면 부팅 직후가 아니라고 본다.
const LATE_S: f64 = 600.0;
/// 측정 시작이 바탕화면보다 이만큼 늦으면 결과를 참고용으로 표시한다(요구서 v1.4 R0).
pub const LATE_START_S: f64 = 30.0;
/// 로그온 작업으로 탐색기보다 먼저 켜졌을 때 탐색기를 기다리는 최대 시간.
const WAIT_SHELL_S: u64 = 60;

pub fn start(app: AppHandle, demo: bool) {
    let shared = app.state::<Arc<Shared>>().inner().clone();
    if shared.running.swap(true, Ordering::SeqCst) {
        if demo {
            ui::show_overlay(&app);
        }
        return;
    }
    thread::spawn(move || {
        crate::update::notify(&app);
        run(&app, &shared, demo);
        shared.running.store(false, Ordering::SeqCst);
        // 측정이 끝난 뒤에만 업데이트를 확인한다(6시간 간격).
        crate::update::notify(&app);
        crate::update::check(&app, false);
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

/// 시스템 로그의 부팅 종류·시각으로 추정값을 바꾼다(요구서 F8 v1.4).
#[cfg(windows)]
fn apply_boot_info(rec: &mut BootRecord, info: Option<startup::windows::BootInfo>) {
    let Some(b) = info else { return };
    let anchor = rec.anchor_time as f64;
    rec.boot_source = "event".into();
    rec.boot_time = b.boot_time;
    rec.boot_kind = match b.boot_type {
        1 => "fast_startup",
        2 => "resume",
        _ => "cold",
    }
    .into();
    rec.power_to_desktop_s = round1((anchor - b.boot_time as f64).max(0.0));
    if let Some(l) = b.logon_time {
        rec.power_to_logon_s = Some(round1(l.saturating_sub(b.boot_time) as f64));
        rec.logon_to_desktop_s = Some(round1((anchor - l as f64).max(0.0)));
    }
}

#[allow(clippy::too_many_arguments)]
fn make_view(
    judge: &Judge,
    t: f64,
    demo: bool,
    phase: &str,
    hit: Option<network::NetHit>,
    background: Vec<BackgroundView>,
    top_advice: Option<Advice>,
    measure_start_s: f64,
    slow_ratio: Option<f64>,
) -> SessionView {
    let checklist = judge.checklist(t);
    let network_note = match (hit, checklist.network) {
        (Some(h), _) if h.first_try => "측정 시작 전에 이미 연결됨".into(),
        (Some(h), _) => format!("연결됨 ({:.1}초)", h.t),
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
        measure_start_s,
        refrozen: judge.refrozen(),
        deferred: judge.deferred_count(),
        slow_ratio,
    }
}

fn run(app: &AppHandle, shared: &Shared, demo: bool) {
    let Some(mut col) = Collector::new() else {
        ui::show_details(app);
        return;
    };
    thread::sleep(Duration::from_secs(1));
    let mut first = col.sample();
    let boot = Collector::boot_time();
    let self_uid = first.procs.get(&col.self_pid).and_then(|p| p.uid.clone());
    let mut found = if demo { None } else { find_anchor(&first.procs, boot, &self_uid) };
    // 작업 스케줄러 로그온 작업은 탐색기보다 먼저 켜질 수 있다.
    let mut waited = 0;
    while !demo && found.is_none() && waited < WAIT_SHELL_S {
        thread::sleep(Duration::from_secs(1));
        waited += 1;
        first = col.sample();
        found = find_anchor(&first.procs, boot, &self_uid);
    }
    let now = now_f();
    let anchor = found.unwrap_or(now);
    let measure_start_s = if demo { 0.0 } else { round1((now - anchor).max(0.0)) };
    if !demo && now - anchor > LATE_S {
        ui::show_details(app);
        // 측정하지 않을 때도 예약 작업 목록을 갱신해 둔다(업데이트 직후 새 필드를 다음 부팅에 쓰도록).
        startup::refresh_cache(&shared.store);
        return;
    }

    let self_exe = std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    let items = startup::collect(&shared.store, &self_exe);
    let history = shared.store.load_history();
    // v1.4 이전 기록은 45초만 봐서 늦게 실행되는 항목도 미실행으로 남았다. 10분 관찰 기록만 쓴다.
    let skip_wait: HashSet<String> = history
        .iter()
        .rev()
        .find(|r| r.v >= RECORD_V)
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

    let net: Arc<Mutex<Option<network::NetHit>>> = Arc::new(Mutex::new(None));
    network::spawn(anchor, net.clone());
    // 체감 반응 지표(F12): 측정 내내 2초마다 잰다. 화면 배율은 직전 측정의 기준값으로 계산한다.
    let probe_file = probe::probe_file(shared.store.dir());
    let probe_samples: Arc<Mutex<Vec<[f64; 4]>>> = Arc::new(Mutex::new(Vec::new()));
    let probe_stop = Arc::new(AtomicBool::new(false));
    probe::spawn(anchor, probe_file.clone(), probe_samples.clone(), probe_stop.clone());
    let prev_baseline = shared.store.load_settings().probe_baseline;
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
    let mut tail_done = false;
    let mut was_frozen = false;
    loop {
        // 땡! 뒤 늦은 항목만 지켜보는 구간은 부하를 줄이려고 2초 간격으로 본다.
        next += Duration::from_secs(if tail_done { 2 } else { 1 });
        if let Some(wait) = next.checked_duration_since(Instant::now()) {
            thread::sleep(wait);
        }
        let s = col.sample();
        let t = now_f() - anchor;
        let hit = net.lock().ok().and_then(|g| *g);
        let slow_ratio = prev_baseline.and_then(|b| {
            let s = probe_samples.lock().ok()?;
            let tail = &s[s.len().saturating_sub(5)..];
            probe::smoothed(tail, &b).last().map(|x| round1(x.1))
        });
        let net_s = network::judge_time(hit);
        judge.tick(t, &s.procs, net_s, s.sys_cpu);
        self_cpu_sum += s.self_cpu;
        samples += 1;
        let bg = background(&s.procs, &judge);
        bg_seen.extend(bg.iter().map(|b| b.name.clone()));

        tail_done = judge.ready_declared_t.is_some_and(|d| t >= d + TAIL_S);
        // 데모는 지금 떠 있지 않은 항목이 앞으로 실행될 일이 없으므로 기다리지 않는다.
        let finished = tail_done && (demo || judge.watch_done(t) || t >= th::OBSERVE_CAP);
        let frozen = judge.refrozen();
        if frozen && !was_frozen {
            ui::show_overlay(app);
        }
        was_frozen = frozen;
        if !finished {
            let phase = if judge.ready_s.is_none() {
                "preparing"
            } else if tail_done {
                "watching"
            } else {
                "ready"
            };
            publish(app, shared, make_view(&judge, t, demo, phase, hit, bg, None, measure_start_s, slow_ratio));
            continue;
        }

        let ready_s = judge.ready_s.unwrap_or(round1(t));
        let late_start = !demo && measure_start_s >= LATE_START_S;
        let mut rec = BootRecord {
            v: RECORD_V,
            os: platform::OS.into(),
            boot_kind: boot_kind(anchor, boot, demo),
            boot_source: if demo { String::new() } else if cfg!(windows) { "estimate".into() } else { "sysinfo".into() },
            boot_time: boot,
            anchor_time: anchor as u64,
            power_to_desktop_s: if demo { 0.0 } else { round1(anchor - boot as f64) },
            event100: None,
            ready_s,
            forced: judge.forced,
            grade: advice::grade(ready_s, judge.forced),
            network_s: network::record_time(hit),
            network_before_start: hit.is_some_and(|h| h.first_try),
            aux_idle_s: judge.aux_s,
            self_cpu_avg: round1(self_cpu_sum / samples.max(1) as f64),
            // 늦게 시작한 측정에서는 어떤 항목이 시점을 정했는지 믿을 수 없다.
            bottleneck: if late_start { None } else { judge.bottleneck() },
            items: judge.item_records(),
            background: bg_seen.into_iter().collect(),
            advice: vec![],
            measure_start_s,
            late_start,
            observe_until_s: round1(t),
            all_launched_s: judge.all_launched_s(),
            refreeze: judge.finish_refreeze(t),
            ..Default::default()
        };
        #[cfg(windows)]
        if !demo {
            rec.event100 = startup::windows::read_event100(boot);
            apply_boot_info(&mut rec, startup::windows::read_boot_info(anchor as u64));
        }
        // 반응 지표 마무리: 표본 수집을 멈추고, 지금(측정 직후)의 평소 값을 기준값으로 잰다.
        probe_stop.store(true, Ordering::SeqCst);
        let base = probe::baseline(&probe_file);
        let _ = std::fs::remove_file(&probe_file);
        let samples = probe_samples.lock().map(|v| v.clone()).unwrap_or_default();
        rec.probe = Some(probe::finish(samples, base));
        if let Some(b) = base {
            let mut st = shared.store.load_settings();
            st.probe_baseline = Some(b);
            shared.store.save_settings(&st);
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
        publish(app, shared, make_view(&judge, t, demo, "done", hit, bg, rec.advice.first().cloned(), measure_start_s, None));
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
    let ready = view.ready_s.is_some() && !view.refrozen;
    let changed = match shared.view.lock() {
        Ok(mut v) => {
            let prev = v.as_ref().map(|p| p.ready_s.is_some() && !p.refrozen);
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
