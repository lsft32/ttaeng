//! 판정 엔진. 시간과 프로세스 표만 입력받는 순수 로직이라 단위 테스트로 검증한다.

use crate::model::*;
use std::collections::{HashMap, HashSet, VecDeque};

pub mod th {
    /// 5초 평균 CPU(코어 1개 기준 %). Windows CPU 시간은 약 15.6ms 단위라 1초 표본이 계단식으로 튀어 평균으로 본다.
    pub const QUIET_CPU: f64 = 8.0;
    pub const QUIET_DISK_BPS: f64 = 1_048_576.0;
    pub const QUIET_HOLD: f64 = 5.0;
    pub const MIN_OBSERVE: f64 = 3.0;
    pub const REACT_CPU: f64 = 20.0;
    pub const REACT_HOLD: f64 = 3.0;
    pub const NOT_LAUNCHED_AT: f64 = 45.0;
    pub const PERSISTENT_AFTER: f64 = 60.0;
    pub const NET_TIMEOUT: f64 = 30.0;
    pub const HARD_CAP: f64 = 300.0;
    pub const READY_HOLD: f64 = 3.0;
    pub const AUX_CPU: f64 = 20.0;
    pub const AUX_ACCUM: f64 = 10.0;
    pub const DETECT_WINDOW: f64 = 45.0;
    pub const ROOT_LOOKBACK: f64 = 120.0;
    pub const DETECT_LOOKBACK: f64 = 5.0;
    /// 1초 간격 조회의 오차 흡수용.
    pub const EPS: f64 = 0.05;
}

pub struct DetectRules {
    pub shell_files: Vec<String>,
    pub system_prefixes: Vec<String>,
    pub self_pid: u32,
    pub self_uid: Option<String>,
}

struct WinSample {
    dt: f64,
    cpu: f64,
    disk: f64,
    child: bool,
}

struct Tracked {
    item: StartupItem,
    state: ItemStateKind,
    /// 직전 부팅에서 미실행이라 기다리지 않는 항목. 실행되면 일반 항목처럼 추적한다.
    optional: bool,
    pids: HashSet<u32>,
    launch_s: Option<f64>,
    observe_s: Option<f64>,
    /// 최근 5초 판정 창.
    window: VecDeque<WinSample>,
    react_start: Option<f64>,
    settle_s: Option<f64>,
    /// 판정을 막지 않게 된 시각.
    met_s: Option<f64>,
    cur_cpu: f64,
    peak_cpu: f64,
    cpu_seconds: f64,
    disk_bytes: f64,
}

impl Tracked {
    fn new(item: StartupItem, optional: bool) -> Self {
        let untrackable = item.matcher == Matcher::None;
        Self {
            item,
            state: if untrackable { ItemStateKind::Untrackable } else { ItemStateKind::Waiting },
            optional,
            pids: HashSet::new(),
            launch_s: None,
            observe_s: None,
            window: VecDeque::new(),
            react_start: None,
            settle_s: None,
            met_s: if untrackable { Some(0.0) } else { None },
            cur_cpu: 0.0,
            peak_cpu: 0.0,
            cpu_seconds: 0.0,
            disk_bytes: 0.0,
        }
    }

    fn blocking(&self) -> bool {
        match self.state {
            ItemStateKind::Waiting => !self.optional,
            ItemStateKind::Running => true,
            _ => false,
        }
    }

    fn matches(&self, p: &ProcInfo) -> bool {
        match &self.item.matcher {
            Matcher::Path(path) => p.exe_lc.as_deref() == Some(path.as_str()),
            Matcher::FileName(f) => &p.file_lc == f,
            Matcher::None => false,
        }
    }
}

pub struct Judge {
    anchor: f64,
    root_lookback: f64,
    rules: DetectRules,
    items: Vec<Tracked>,
    seen: HashSet<u32>,
    /// pid → (항목 인덱스, 프로세스 시작 시각). 종료된 pid도 남겨 고아 자식을 연결한다.
    claimed: HashMap<u32, (usize, u64)>,
    last_t: Option<f64>,
    net_ok_s: Option<f64>,
    ready_since: Option<f64>,
    pub ready_s: Option<f64>,
    pub ready_declared_t: Option<f64>,
    pub forced: bool,
    aux_accum: f64,
    pub aux_s: Option<f64>,
}

impl Judge {
    pub fn new(
        anchor: f64,
        items: Vec<StartupItem>,
        skip_wait: &HashSet<String>,
        rules: DetectRules,
        root_lookback: f64,
    ) -> Self {
        let items = items
            .into_iter()
            .map(|i| {
                let optional = skip_wait.contains(&i.key);
                Tracked::new(i, optional)
            })
            .collect();
        Self {
            anchor,
            root_lookback,
            rules,
            items,
            seen: HashSet::new(),
            claimed: HashMap::new(),
            last_t: None,
            net_ok_s: None,
            ready_since: None,
            ready_s: None,
            ready_declared_t: None,
            forced: false,
            aux_accum: 0.0,
            aux_s: None,
        }
    }

    fn rel(&self, start: u64) -> f64 {
        start as f64 - self.anchor
    }

    fn claim(&mut self, pid: u32, idx: usize, start: u64) {
        self.claimed.insert(pid, (idx, start));
        self.items[idx].pids.insert(pid);
    }

    fn is_detect_candidate(&self, p: &ProcInfo, procs: &ProcTable) -> bool {
        if p.pid == self.rules.self_pid || self.rel(p.start) < -th::DETECT_LOOKBACK {
            return false;
        }
        let Some(exe) = &p.exe_lc else { return false };
        if self.rules.system_prefixes.iter().any(|s| exe.starts_with(s.as_str())) {
            return false;
        }
        if let (Some(a), Some(b)) = (&p.uid, &self.rules.self_uid) {
            if a != b {
                return false;
            }
        }
        match p.ppid.and_then(|pp| procs.get(&pp)) {
            None => true,
            Some(parent) => self.rules.shell_files.iter().any(|s| *s == parent.file_lc),
        }
    }

    fn grow_trees(&mut self, procs: &ProcTable, new_pids: &HashSet<u32>, new_child: &mut HashSet<usize>) {
        loop {
            let mut adds = Vec::new();
            for (pid, p) in procs {
                if self.claimed.contains_key(pid) {
                    continue;
                }
                if let Some(&(idx, _)) = p.ppid.and_then(|pp| self.claimed.get(&pp)) {
                    adds.push((*pid, idx, p.start));
                }
            }
            if adds.is_empty() {
                break;
            }
            for (pid, idx, start) in adds {
                if new_pids.contains(&pid) && !self.items[idx].pids.is_empty() {
                    new_child.insert(idx);
                }
                self.claim(pid, idx, start);
            }
        }
    }

    pub fn tick(&mut self, t: f64, procs: &ProcTable, net_ok_s: Option<f64>, sys_cpu: f64) {
        let prev = self.last_t.unwrap_or(t - 1.0);
        let dt = (t - prev).max(0.001);
        self.last_t = Some(t);
        self.net_ok_s = net_ok_s;
        let declared = self.ready_s.is_some();

        // pid 재사용: 같은 번호인데 시작 시각이 다르면 다른 프로세스다.
        let stale: Vec<u32> = self
            .claimed
            .iter()
            .filter(|(pid, (_, start))| procs.get(pid).is_some_and(|p| p.start != *start))
            .map(|(pid, _)| *pid)
            .collect();
        for pid in stale {
            if let Some((idx, _)) = self.claimed.remove(&pid) {
                self.items[idx].pids.remove(&pid);
            }
            self.seen.remove(&pid);
        }

        let new_pids: HashSet<u32> = procs.keys().filter(|p| !self.seen.contains(p)).copied().collect();
        let mut new_child: HashSet<usize> = HashSet::new();

        // 1. 등록 항목의 루트 프로세스
        let mut roots = Vec::new();
        for (pid, p) in procs {
            if self.claimed.contains_key(pid) || self.rel(p.start) < -self.root_lookback {
                continue;
            }
            if let Some(idx) = self.items.iter().position(|tr| tr.matches(p)) {
                roots.push((*pid, idx, p.start));
            }
        }
        for (pid, idx, start) in roots {
            if new_pids.contains(&pid) && !self.items[idx].pids.is_empty() {
                new_child.insert(idx);
            }
            self.claim(pid, idx, start);
        }

        // 2. 트리 확장 → 3. 자동 감지 → 감지 항목의 자식까지 다시 확장
        self.grow_trees(procs, &new_pids, &mut new_child);
        if !declared && t <= th::DETECT_WINDOW + th::EPS {
            let mut detected: Vec<(u32, String, String, u64)> = Vec::new();
            for pid in &new_pids {
                let p = &procs[pid];
                if !self.claimed.contains_key(pid) && self.is_detect_candidate(p, procs) {
                    detected.push((*pid, p.exe_lc.clone().unwrap_or_default(), p.name.clone(), p.start));
                }
            }
            for (pid, exe, name, start) in detected {
                let idx = match self.items.iter().position(|tr| tr.item.key == exe) {
                    Some(i) => i,
                    None => {
                        let display = name.trim_end_matches(".exe").trim_end_matches(".EXE").to_string();
                        self.items.push(Tracked::new(
                            StartupItem::new(&display, Source::Detected, Matcher::Path(exe)),
                            false,
                        ));
                        self.items.len() - 1
                    }
                };
                self.claim(pid, idx, start);
            }
            self.grow_trees(procs, &new_pids, &mut new_child);
        }
        self.seen.extend(new_pids.iter().copied());

        // 4. 항목별 측정과 상태 전이
        for idx in 0..self.items.len() {
            let anchor = self.anchor;
            let tr = &mut self.items[idx];
            let alive: Vec<&ProcInfo> = tr.pids.iter().filter_map(|pid| procs.get(pid)).collect();
            let cpu: f64 = alive.iter().map(|p| p.cpu as f64).sum();
            let disk: f64 = alive.iter().map(|p| p.disk as f64).sum();
            tr.cur_cpu = cpu;
            tr.peak_cpu = tr.peak_cpu.max(cpu);
            tr.cpu_seconds += cpu / 100.0 * dt;
            tr.disk_bytes += disk;

            match tr.state {
                ItemStateKind::Untrackable | ItemStateKind::NotLaunched | ItemStateKind::Persistent => {}
                ItemStateKind::Waiting => {
                    if !alive.is_empty() {
                        let first = alive.iter().map(|p| p.start).min().unwrap_or(0);
                        tr.state = ItemStateKind::Running;
                        tr.launch_s = Some(first as f64 - anchor);
                        tr.observe_s = Some(t);
                    } else if t >= th::NOT_LAUNCHED_AT - th::EPS {
                        tr.state = ItemStateKind::NotLaunched;
                        tr.met_s = Some(if tr.optional { 0.0 } else { t });
                    }
                }
                ItemStateKind::Running => {
                    let observe = tr.observe_s.unwrap_or(t);
                    let launch = tr.launch_s.unwrap_or(observe);
                    if alive.is_empty() {
                        tr.state = ItemStateKind::Exited;
                        tr.settle_s = Some(prev.max(launch));
                        tr.met_s = tr.settle_s;
                    } else {
                        tr.window.push_back(WinSample { dt, cpu, disk, child: new_child.contains(&idx) });
                        let mut span: f64 = tr.window.iter().map(|s| s.dt).sum();
                        while tr.window.len() > 1 && span - tr.window[0].dt >= th::QUIET_HOLD - th::EPS {
                            span -= tr.window.pop_front().map(|s| s.dt).unwrap_or(0.0);
                        }
                        let full = span >= th::QUIET_HOLD - th::EPS && t - observe >= th::MIN_OBSERVE - th::EPS;
                        let mean_cpu = tr.window.iter().map(|s| s.cpu * s.dt).sum::<f64>() / span;
                        let disk_rate = tr.window.iter().map(|s| s.disk).sum::<f64>() / span;
                        let child = tr.window.iter().any(|s| s.child);
                        if full && mean_cpu < th::QUIET_CPU && disk_rate < th::QUIET_DISK_BPS && !child {
                            tr.state = ItemStateKind::Settled;
                            // 조건을 처음 만족한 5초 창의 시작을 안정 시점으로 본다.
                            tr.settle_s = Some((t - span).max(launch));
                            tr.met_s = tr.settle_s;
                            tr.window.clear();
                        }
                        if tr.state == ItemStateKind::Running && t - launch.max(0.0) >= th::PERSISTENT_AFTER - th::EPS {
                            tr.state = ItemStateKind::Persistent;
                            tr.met_s = Some(t);
                        }
                    }
                }
                ItemStateKind::Settled | ItemStateKind::Exited => {
                    if declared {
                        continue;
                    }
                    if cpu >= th::REACT_CPU {
                        let rs = *tr.react_start.get_or_insert(prev);
                        if t - rs >= th::REACT_HOLD - th::EPS {
                            tr.state = ItemStateKind::Running;
                            tr.settle_s = None;
                            tr.met_s = None;
                            tr.react_start = None;
                            tr.window.clear();
                        }
                    } else {
                        tr.react_start = None;
                    }
                }
            }
        }

        // 5. 체크리스트와 사용 가능 판정
        if !declared {
            let checklist = self.checklist(t);
            let all = checklist.items == Check::Ok && checklist.network != Check::Wait;
            if all {
                let since = *self.ready_since.get_or_insert(t);
                if t - since >= th::READY_HOLD - th::EPS {
                    self.ready_s = Some(round1(self.met_max(t)));
                    self.ready_declared_t = Some(t);
                }
            } else {
                self.ready_since = None;
            }
            if self.ready_s.is_none() && t >= th::HARD_CAP - th::EPS {
                self.ready_s = Some(th::HARD_CAP);
                self.ready_declared_t = Some(t);
                self.forced = true;
            }
        }

        // 6. 보조 지표
        if sys_cpu <= th::AUX_CPU {
            self.aux_accum += dt;
        }
        if self.aux_s.is_none() && self.aux_accum >= th::AUX_ACCUM - th::EPS {
            self.aux_s = Some(round1(t));
        }
    }

    fn network_met(&self, t: f64) -> Option<f64> {
        match self.net_ok_s {
            Some(s) => Some(s),
            None if t >= th::NET_TIMEOUT - th::EPS => Some(th::NET_TIMEOUT),
            None => None,
        }
    }

    fn met_max(&self, t: f64) -> f64 {
        let items = self.items.iter().filter_map(|tr| tr.met_s).fold(0.0_f64, f64::max);
        items.max(self.network_met(t).unwrap_or(0.0)).max(0.0)
    }

    pub fn checklist(&self, t: f64) -> Checklist {
        let items_ok = self.items.iter().all(|tr| !tr.blocking());
        let network = if self.net_ok_s.is_some() {
            Check::Ok
        } else if t >= th::NET_TIMEOUT - th::EPS {
            Check::Skip
        } else {
            Check::Wait
        };
        Checklist {
            desktop: Check::Ok,
            items: if items_ok { Check::Ok } else { Check::Wait },
            network,
        }
    }

    pub fn progress(&self) -> Progress {
        let counted: Vec<&Tracked> =
            self.items.iter().filter(|tr| tr.state != ItemStateKind::Untrackable).collect();
        Progress {
            done: counted.iter().filter(|tr| !tr.blocking()).count(),
            total: counted.len(),
        }
    }

    pub fn item_views(&self) -> Vec<ItemView> {
        let mut v: Vec<(bool, f64, ItemView)> = self
            .items
            .iter()
            .map(|tr| {
                (
                    !tr.blocking(),
                    tr.launch_s.unwrap_or(f64::MAX),
                    ItemView {
                        key: tr.item.key.clone(),
                        name: tr.item.name.clone(),
                        source: tr.item.source,
                        state: tr.state,
                        cpu: round1(tr.cur_cpu),
                        launch_s: tr.launch_s.map(round1),
                        settle_s: tr.settle_s.map(round1),
                    },
                )
            })
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        v.into_iter().map(|x| x.2).collect()
    }

    pub fn item_records(&self) -> Vec<ItemRecord> {
        self.items
            .iter()
            .map(|tr| ItemRecord {
                key: tr.item.key.clone(),
                name: tr.item.name.clone(),
                source: tr.item.source,
                // 측정이 끝날 때까지 결론이 안 난 항목은 지속형으로 남긴다.
                state: match tr.state {
                    ItemStateKind::Running => ItemStateKind::Persistent,
                    ItemStateKind::Waiting => ItemStateKind::NotLaunched,
                    s => s,
                },
                launch_s: tr.launch_s.map(round1),
                settle_s: tr.settle_s.map(round1),
                peak_cpu: round1(tr.peak_cpu),
                cpu_seconds: round1(tr.cpu_seconds),
                disk_mb: round1(tr.disk_bytes / 1_048_576.0),
            })
            .collect()
    }

    /// 사용 가능 시점을 결정한 항목. 네트워크가 더 늦었거나 강제 판정이면 None.
    pub fn bottleneck(&self) -> Option<String> {
        let ready = self.ready_s?;
        if self.forced {
            return None;
        }
        let best = self
            .items
            .iter()
            .filter(|tr| matches!(tr.state, ItemStateKind::Settled | ItemStateKind::Exited | ItemStateKind::Persistent))
            .filter_map(|tr| tr.met_s.map(|m| (m, tr)))
            .max_by(|a, b| a.0.total_cmp(&b.0))?;
        let net = self.net_ok_s.unwrap_or(th::NET_TIMEOUT);
        if (round1(best.0) - ready).abs() < 0.11 && best.0 >= net {
            Some(best.1.item.key.clone())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANCHOR: u64 = 1_000_000;

    fn rules() -> DetectRules {
        DetectRules {
            shell_files: vec!["explorer.exe".into()],
            system_prefixes: vec!["c:\\windows\\".into()],
            self_pid: 1,
            self_uid: None,
        }
    }

    fn proc(pid: u32, ppid: u32, exe: &str, start_rel: i64, cpu: f32) -> ProcInfo {
        let file = exe.rsplit('\\').next().unwrap().to_string();
        ProcInfo {
            pid,
            ppid: Some(ppid),
            exe_lc: Some(exe.to_string()),
            file_lc: file.clone(),
            name: file,
            start: (ANCHOR as i64 + start_rel) as u64,
            cpu,
            disk: 0,
            uid: None,
        }
    }

    fn table(ps: Vec<ProcInfo>) -> ProcTable {
        ps.into_iter().map(|p| (p.pid, p)).collect()
    }

    fn explorer() -> ProcInfo {
        proc(10, 4, "c:\\windows\\explorer.exe", 0, 1.0)
    }

    fn item(path: &str) -> StartupItem {
        StartupItem::new("App", Source::RunKey, Matcher::Path(path.into()))
    }

    fn judge(items: Vec<StartupItem>) -> Judge {
        Judge::new(ANCHOR as f64, items, &HashSet::new(), rules(), th::ROOT_LOOKBACK)
    }

    const APP: &str = "c:\\apps\\app.exe";

    #[test]
    fn busy_then_quiet_item_settles_and_ready() {
        let mut j = judge(vec![item(APP)]);
        for t in 1..=40 {
            let cpu = if t <= 10 { 50.0 } else { 0.5 };
            let procs = table(vec![explorer(), proc(20, 10, APP, 2, cpu)]);
            j.tick(t as f64, &procs, Some(4.0), 50.0);
            if j.ready_s.is_some() {
                break;
            }
        }
        // 조용해진 구간은 t=10 직후부터 → settle_s = 10
        assert_eq!(j.ready_s, Some(10.0));
        assert_eq!(j.bottleneck().as_deref(), Some(APP));
        assert_eq!(j.item_records()[0].state, ItemStateKind::Settled);
    }

    #[test]
    fn quiet_hold_required_before_settle() {
        let mut j = judge(vec![item(APP)]);
        let procs = table(vec![explorer(), proc(20, 10, APP, -30, 0.0)]);
        for t in 1..=4 {
            j.tick(t as f64, &procs, Some(1.0), 0.0);
        }
        assert_eq!(j.item_views()[0].state, ItemStateKind::Running);
        for t in 5..=7 {
            j.tick(t as f64, &procs, Some(1.0), 0.0);
        }
        assert_eq!(j.item_views()[0].state, ItemStateKind::Settled);
    }

    #[test]
    fn not_launched_after_45s_and_does_not_block() {
        let mut j = judge(vec![item(APP)]);
        let procs = table(vec![explorer()]);
        for t in 1..=44 {
            j.tick(t as f64, &procs, Some(1.0), 0.0);
        }
        assert!(j.ready_s.is_none());
        for t in 45..=48 {
            j.tick(t as f64, &procs, Some(1.0), 0.0);
        }
        assert_eq!(j.ready_s, Some(45.0));
    }

    #[test]
    fn optional_item_from_previous_boot_does_not_wait() {
        let skip: HashSet<String> = [APP.to_string()].into();
        let mut j = Judge::new(ANCHOR as f64, vec![item(APP)], &skip, rules(), th::ROOT_LOOKBACK);
        let procs = table(vec![explorer()]);
        for t in 1..=5 {
            j.tick(t as f64, &procs, Some(1.0), 0.0);
        }
        assert_eq!(j.ready_s, Some(1.0));
    }

    #[test]
    fn steady_background_draw_settles() {
        // 대기 중에도 6% 안팎을 꾸준히 쓰는 상주 앱(실측: RaiDrive.Mount 6.4%)
        let mut j = judge(vec![item(APP)]);
        for t in 1..=20 {
            let cpu = if t <= 8 { 60.0 } else if t % 2 == 0 { 5.5 } else { 7.0 };
            j.tick(t as f64, &table(vec![explorer(), proc(20, 10, APP, 0, cpu)]), Some(1.0), 0.0);
        }
        let v = &j.item_views()[0];
        assert_eq!(v.state, ItemStateKind::Settled);
        assert_eq!(v.settle_s, Some(8.0));
    }

    #[test]
    fn windows_quantized_idle_samples_settle() {
        // 실측 RaiDrive.Mount 1초 표본(평균 5.2%, 가끔 25% 튐)
        let s = [4.7, 3.1, 4.7, 25.0, 3.1, 0.0, 6.2, 9.4, 0.0, 3.1, 3.1, 0.0, 0.0, 17.2, 9.4, 6.2, 3.1, 0.0, 1.6, 4.7];
        let mut j = judge(vec![item(APP)]);
        for (i, cpu) in s.iter().enumerate() {
            j.tick((i + 1) as f64, &table(vec![explorer(), proc(20, 10, APP, 0, *cpu as f32)]), Some(1.0), 0.0);
        }
        assert_eq!(j.item_views()[0].state, ItemStateKind::Settled);
        assert!(j.item_views()[0].settle_s.unwrap() <= 5.0);
    }

    #[test]
    fn bursty_load_is_not_quiet() {
        let mut j = judge(vec![item(APP)]);
        for t in 1..=20 {
            let cpu = if t % 2 == 0 { 2.0 } else { 30.0 };
            j.tick(t as f64, &table(vec![explorer(), proc(20, 10, APP, 0, cpu)]), Some(1.0), 0.0);
        }
        assert_eq!(j.item_views()[0].state, ItemStateKind::Running);
    }

    #[test]
    fn launcher_exit_counts_as_settled() {
        let mut j = judge(vec![item(APP)]);
        for t in 1..=3 {
            j.tick(t as f64, &table(vec![explorer(), proc(20, 10, APP, 0, 30.0)]), Some(1.0), 0.0);
        }
        for t in 4..=8 {
            j.tick(t as f64, &table(vec![explorer()]), Some(1.0), 0.0);
        }
        assert_eq!(j.item_records()[0].state, ItemStateKind::Exited);
        assert_eq!(j.ready_s, Some(3.0));
    }

    #[test]
    fn orphan_child_stays_in_tree_after_parent_exits() {
        let mut j = judge(vec![item(APP)]);
        let child = "c:\\apps\\worker.exe";
        j.tick(1.0, &table(vec![explorer(), proc(20, 10, APP, 0, 10.0), proc(21, 20, child, 1, 10.0)]), Some(1.0), 0.0);
        // 런처 종료, 자식만 바쁨 → 항목은 계속 실행 중
        for t in 2..=10 {
            j.tick(t as f64, &table(vec![explorer(), proc(21, 20, child, 1, 40.0)]), Some(1.0), 0.0);
        }
        assert_eq!(j.item_views()[0].state, ItemStateKind::Running);
        assert!(j.item_views()[0].cpu >= 40.0);
    }

    #[test]
    fn persistent_after_60s() {
        let mut j = judge(vec![item(APP)]);
        for t in 1..=70 {
            j.tick(t as f64, &table(vec![explorer(), proc(20, 10, APP, 1, 50.0)]), Some(1.0), 0.0);
        }
        assert_eq!(j.item_views()[0].state, ItemStateKind::Persistent);
        assert_eq!(j.ready_s, Some(61.0));
        assert_eq!(j.bottleneck().as_deref(), Some(APP));
    }

    #[test]
    fn reactivation_before_ready() {
        let mut j = judge(vec![item(APP), item("c:\\apps\\slow.exe")]);
        let slow = proc(30, 10, "c:\\apps\\slow.exe", 0, 50.0);
        for t in 1..=8 {
            j.tick(t as f64, &table(vec![explorer(), proc(20, 10, APP, 0, 0.0), slow.clone()]), Some(1.0), 0.0);
        }
        assert_eq!(j.item_views().iter().find(|v| v.key == APP).unwrap().state, ItemStateKind::Settled);
        for t in 9..=12 {
            j.tick(t as f64, &table(vec![explorer(), proc(20, 10, APP, 0, 30.0), slow.clone()]), Some(1.0), 0.0);
        }
        assert_eq!(j.item_views().iter().find(|v| v.key == APP).unwrap().state, ItemStateKind::Running);
    }

    #[test]
    fn network_blocks_until_timeout() {
        let mut j = judge(vec![]);
        for t in 1..=29 {
            j.tick(t as f64, &table(vec![explorer()]), None, 0.0);
        }
        assert!(j.ready_s.is_none());
        for t in 30..=33 {
            j.tick(t as f64, &table(vec![explorer()]), None, 0.0);
        }
        assert_eq!(j.ready_s, Some(30.0));
        assert_eq!(j.checklist(33.0).network, Check::Skip);
    }

    #[test]
    fn detects_unlisted_app_launched_by_shell() {
        let mut j = judge(vec![]);
        let procs = table(vec![
            explorer(),
            proc(40, 10, "c:\\users\\u\\appdata\\tray.exe", 3, 20.0),
            proc(41, 10, "c:\\windows\\system32\\ctfmon.exe", 3, 20.0),
            proc(42, 10, "c:\\apps\\old.exe", -60, 20.0),
        ]);
        j.tick(1.0, &procs, Some(1.0), 0.0);
        let views = j.item_views();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].source, Source::Detected);
        assert_eq!(views[0].name, "tray");
    }

    #[test]
    fn pid_reuse_is_not_counted() {
        let mut j = judge(vec![item(APP)]);
        j.tick(1.0, &table(vec![explorer(), proc(20, 10, APP, 0, 50.0)]), Some(1.0), 0.0);
        j.tick(2.0, &table(vec![explorer()]), Some(1.0), 0.0);
        // 같은 pid 20이 다른 프로그램으로 재사용됨
        j.tick(3.0, &table(vec![explorer(), proc(20, 4, "c:\\windows\\system32\\svc.exe", 3, 90.0)]), Some(1.0), 0.0);
        assert_eq!(j.item_views()[0].cpu, 0.0);
    }

    #[test]
    fn flapping_item_still_ends_by_persistent_rule() {
        let mut j = judge(vec![item(APP), item("c:\\apps\\other.exe")]);
        for t in 1..=120 {
            let cpu = if (t / 6) % 2 == 0 { 0.0 } else { 40.0 };
            let procs = table(vec![
                explorer(),
                proc(20, 10, APP, 0, cpu),
                proc(30, 10, "c:\\apps\\other.exe", 0, 50.0),
            ]);
            j.tick(t as f64, &procs, Some(1.0), 0.0);
            if j.ready_s.is_some() {
                break;
            }
        }
        assert!(j.ready_s.is_some_and(|r| r <= 61.0));
        assert!(!j.forced);
    }

    #[test]
    fn aux_idle_accumulates() {
        let mut j = judge(vec![]);
        for t in 1..=20 {
            let cpu = if t % 2 == 0 { 10.0 } else { 90.0 };
            j.tick(t as f64, &table(vec![explorer()]), Some(1.0), cpu);
        }
        assert_eq!(j.aux_s, Some(20.0));
    }
}
