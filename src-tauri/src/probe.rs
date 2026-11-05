//! 체감 반응 지표(요구서 F12). 시작프로그램이 조용해졌는지와 별개로, 실제로 PC 반응이 느렸는지를 잰다.
//! 측정 하나는 수 ms라 부하는 무시할 수준이다.

use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const INTERVAL_S: u64 = 2;
/// 이 배율 이하면 평소 수준으로 본다.
pub const RECOVER_RATIO: f64 = 1.5;
/// 반응 회복으로 보려면 이 시간 동안 이어져야 한다.
pub const RECOVER_HOLD_S: f64 = 10.0;
/// 기준값이 너무 작아 배율이 과장되는 것을 막는 최소 기준(ms): 타이머, 계산, 디스크.
/// 개발 PC 유휴 30회 실측: 타이머 중앙 0.54·90% 0.82·최대 1.79ms, 디스크 중앙 5.7·90% 7.8ms.
const FLOOR_MS: [f64; 3] = [1.0, 0.5, 2.0];
/// 잡음(유휴에도 생기는 튐)을 줄이려고 앞뒤 2개씩, 5표본(10초) 중앙값을 쓴다.
const SMOOTH: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    pub timer_ms: f64,
    pub cpu_ms: f64,
    pub io_ms: f64,
}

impl Baseline {
    fn floored(&self) -> [f64; 3] {
        [self.timer_ms.max(FLOOR_MS[0]), self.cpu_ms.max(FLOOR_MS[1]), self.io_ms.max(FLOOR_MS[2])]
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProbeRecord {
    pub baseline: Option<Baseline>,
    /// [t, timer_ms, cpu_ms, io_ms]
    pub samples: Vec<[f64; 4]>,
    pub recovered_s: Option<f64>,
    pub peak_ratio: f64,
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// 1ms 대기를 5번 요청해 늦게 돌아온 평균(ms).
fn timer_ms() -> f64 {
    let mut over = 0.0;
    for _ in 0..5 {
        let s = Instant::now();
        thread::sleep(Duration::from_millis(1));
        over += (ms(s.elapsed()) - 1.0).max(0.0);
    }
    over / 5.0
}

/// 고정된 작은 계산(FNV 해시 20만 회)의 걸린 시간(ms).
fn cpu_ms() -> f64 {
    let s = Instant::now();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for i in 0..200_000u64 {
        h ^= i;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    std::hint::black_box(h);
    ms(s.elapsed())
}

/// 4KB를 쓰고 디스크까지 내려쓴 시간(ms).
fn io_ms(file: &Path) -> f64 {
    let s = Instant::now();
    let ok = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(file)
        .and_then(|mut f| {
            f.write_all(&[0x5a; 4096])?;
            f.sync_all()
        })
        .is_ok();
    // 실패는 -1로 남긴다(NaN은 JSON에 null로 저장돼 다시 읽을 때 오류가 난다).
    if ok { ms(s.elapsed()) } else { -1.0 }
}

pub fn measure(file: &Path) -> [f64; 3] {
    [timer_ms(), cpu_ms(), io_ms(file)]
}

fn median(mut v: Vec<f64>) -> f64 {
    v.retain(|x| x.is_finite() && *x >= 0.0);
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// 측정이 끝난 뒤 10번(약 5초) 재서 중앙값을 기준값으로 쓴다.
pub fn baseline(file: &Path) -> Option<Baseline> {
    let mut m: Vec<[f64; 3]> = Vec::new();
    for _ in 0..10 {
        m.push(measure(file));
        thread::sleep(Duration::from_millis(400));
    }
    let col = |i: usize| median(m.iter().map(|x| x[i]).collect());
    let b = Baseline { timer_ms: col(0), cpu_ms: col(1), io_ms: col(2) };
    let io = if b.io_ms.is_finite() { b.io_ms } else { -1.0 };
    (b.timer_ms.is_finite() && b.cpu_ms.is_finite()).then_some(Baseline { io_ms: io, ..b })
}

pub fn probe_file(dir: &Path) -> PathBuf {
    dir.join("probe.tmp")
}

/// 2초마다 재서 `out`에 [기준 대비 초, 타이머, 계산, 디스크]를 쌓는다. `stop`이 켜지면 끝낸다.
pub fn spawn(anchor: f64, file: PathBuf, out: Arc<Mutex<Vec<[f64; 4]>>>, stop: Arc<AtomicBool>) {
    thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            let [a, b, c] = measure(&file);
            let t = crate::model::now_f() - anchor;
            if let Ok(mut v) = out.lock() {
                v.push([crate::model::round1(t), round2(a), round2(b), round2(c)]);
            }
            thread::sleep(Duration::from_secs(INTERVAL_S));
        }
    });
}

fn round2(v: f64) -> f64 {
    if v.is_finite() { (v * 100.0).round() / 100.0 } else { v }
}

/// 표본 하나의 느려진 정도: 타이머·디스크 배율 중 큰 값. 못 잰 값(음수)은 빼고 본다.
/// 계산 지표는 기록만 하고 판단에서 뺀다. 하이브리드 CPU(P/E 코어)에서는 어느 코어에서 도느냐에 따라
/// 같은 계산이 몇 배 차이 나서, 부하가 아니라 코어 배치를 재게 된다(유휴 데모에서 2.6배·미회복으로 나온 사례).
pub fn ratio(s: &[f64; 4], base: &Baseline) -> f64 {
    let f = base.floored();
    [(s[1], f[0]), (s[3], f[2])]
        .into_iter()
        .filter(|(v, _)| v.is_finite() && *v >= 0.0)
        .map(|(v, b)| v / b)
        .fold(0.0, f64::max)
}

/// 연속 5표본 중앙값으로 잡음을 줄인 느려진 정도. (t, 배율)
pub fn smoothed(samples: &[[f64; 4]], base: &Baseline) -> Vec<(f64, f64)> {
    let r: Vec<f64> = samples.iter().map(|s| ratio(s, base)).collect();
    (0..r.len())
        .map(|i| {
            let lo = i.saturating_sub(SMOOTH);
            let hi = (i + SMOOTH).min(r.len() - 1);
            (samples[i][0], median(r[lo..=hi].to_vec()))
        })
        .collect()
}

/// 느려진 정도가 1.5배 이하로 10초 이상 이어지기 시작한 시각.
pub fn recovered_at(series: &[(f64, f64)]) -> Option<f64> {
    let mut start: Option<f64> = None;
    for &(t, r) in series {
        if r <= RECOVER_RATIO {
            let s = *start.get_or_insert(t);
            if t - s >= RECOVER_HOLD_S - 0.05 {
                return Some(s);
            }
        } else {
            start = None;
        }
    }
    None
}

pub fn finish(samples: Vec<[f64; 4]>, base: Option<Baseline>) -> ProbeRecord {
    let Some(b) = base else {
        return ProbeRecord { baseline: None, samples, recovered_s: None, peak_ratio: 0.0 };
    };
    let series = smoothed(&samples, &b);
    let peak = series.iter().map(|x| x.1).fold(0.0, f64::max);
    ProbeRecord {
        baseline: Some(b),
        recovered_s: recovered_at(&series),
        peak_ratio: round2(peak),
        samples,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: Baseline = Baseline { timer_ms: 0.4, cpu_ms: 0.6, io_ms: 0.8 };

    #[test]
    fn floors_prevent_inflated_ratios() {
        // 타이머 기준 0.4ms → 최소 1ms로 계산. 1.8ms면 1.8배(유휴 최대치 수준이 1.8배로 과장되지 않게)
        assert!((ratio(&[0.0, 1.8, 0.6, 1.0], &B) - 1.8).abs() < 1e-9);
        // 디스크를 못 재면(-1) 타이머로만 판단, 계산 지표(1.2ms = 2배)는 판단에 쓰지 않음
        assert!((ratio(&[0.0, 0.5, 1.2, -1.0], &B) - 0.5).abs() < 1e-9);
        // 디스크 6ms / 최소 기준 2ms = 3배
        assert!((ratio(&[0.0, 0.5, 0.6, 6.0], &B) - 3.0).abs() < 1e-9);
    }

    #[test]
    fn recovery_needs_ten_quiet_seconds_and_ignores_single_spike() {
        // 0~20초 느림(3배), 22초부터 평소 수준, 30초에 한 번 튐(4배)
        let mut s = Vec::new();
        for i in 0..=30 {
            let t = (i * 2) as f64;
            let slow = t <= 20.0 || t == 30.0;
            s.push([t, if slow { 3.0 } else { 0.5 }, 0.6, 1.0]);
        }
        let series = smoothed(&s, &B);
        // 단일 튐은 5표본 중앙값에서 사라지므로 22초부터 회복
        assert_eq!(recovered_at(&series), Some(22.0));
        let rec = finish(s, Some(B));
        assert!(rec.peak_ratio >= 3.0);
    }

    #[test]
    fn measure_runs_and_baseline_is_finite() {
        let dir = std::env::temp_dir().join(format!("ttaeng-probe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = probe_file(&dir);
        let m = measure(&f);
        eprintln!("probe now: timer {:.2}ms cpu {:.2}ms io {:.2}ms", m[0], m[1], m[2]);
        assert!(m.iter().all(|x| x.is_finite() && *x >= 0.0), "{m:?}");
        // 저장 후 다시 읽혀야 한다(NaN 금지)
        let rec = finish(vec![[1.0, m[0], m[1], -1.0]], Some(B));
        let back: ProbeRecord = serde_json::from_str(&serde_json::to_string(&rec).unwrap()).unwrap();
        assert_eq!(back.samples.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
