//! 평가 등급과 추천. 측정값이 근거인 규칙만 둔다.

use crate::model::*;
use crate::probe;

pub fn grade(ready_s: f64, forced: bool) -> String {
    let g = if forced || ready_s > 120.0 {
        "D"
    } else if ready_s > 60.0 {
        "C"
    } else if ready_s > 30.0 {
        "B"
    } else {
        "A"
    };
    g.to_string()
}

fn how_to(os: &str, source: Source) -> &'static str {
    match (os, source) {
        ("windows", Source::LogonTask) => "작업 스케줄러에서 해당 작업의 '로그온할 때' 트리거를 끄세요.",
        ("windows", Source::Detected) => {
            "등록 목록에서 찾지 못하고 실행만 감지된 항목입니다. 프로그램 자체 설정의 '시작 시 실행' 옵션이나 Autoruns로 등록 위치를 확인하세요."
        }
        ("windows", _) => "설정 > 앱 > 시작 프로그램에서 끌 수 있습니다.",
        (_, Source::Detected) => {
            "시스템 설정 > 일반 > 로그인 항목을 확인하고, 없으면 '다시 로그인할 때 윈도우 다시 열기'로 복원된 앱일 수 있습니다."
        }
        _ => "시스템 설정 > 일반 > 로그인 항목에서 끌 수 있습니다.",
    }
}

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    Some(if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 })
}

/// `prev`는 오래된 것부터 정렬된 이전 기록(현재 기록 제외).
pub fn build(rec: &BootRecord, prev: &[BootRecord]) -> Vec<Advice> {
    let mut out = Vec::new();
    let os = rec.os.as_str();

    // R0 측정 지연: 앱이 늦게 켜져 그 전의 진행은 보지 못했다. R1·R4는 근거가 약해 내지 않는다.
    if rec.late_start {
        let fix = if os == "windows" {
            "자동 실행이 Run 키로 등록돼 있으면 실행할 때 작업 스케줄러로 옮기므로 다음 로그온부터는 바로 시작됩니다. 상세 화면 위쪽 '로그인 시 자동 실행' 옆 표시가 계속 'Run 키'라면 체크를 껐다 다시 켜 보세요."
        } else {
            "로그인 항목에서 땡해줘가 켜져 있는지 확인하세요."
        };
        out.push(Advice {
            rule: "R0".into(),
            title: format!("땡해줘가 바탕화면보다 {:.0}초 늦게 측정을 시작해 결과는 참고용입니다", rec.measure_start_s),
            detail: format!("그 전에 끝난 작업의 정확한 시점은 알 수 없어 사용 가능 시간과 등급이 실제와 다를 수 있습니다. {fix}"),
            item: None,
        });
    }

    // R1 병목
    if let Some(key) = rec.bottleneck.as_ref().filter(|_| !rec.late_start) {
        if let Some(it) = rec.items.iter().find(|i| &i.key == key) {
            let met = it.settle_s.unwrap_or(rec.ready_s);
            if rec.ready_s >= 15.0 {
                let reason = if it.state == ItemStateKind::Persistent {
                    "60초가 지나도 작업을 계속해 지속형으로 분류될 때까지 판정을 붙잡았습니다.".to_string()
                } else {
                    format!(
                        "실행 {}초 → 안정 {}초.",
                        it.launch_s.map(|v| format!("{v:.1}")).unwrap_or("-".into()),
                        format!("{met:.1}")
                    )
                };
                out.push(Advice {
                    rule: "R1".into(),
                    title: format!("'{}'이(가) 사용 가능 시점({:.1}초)을 정했습니다", it.name, rec.ready_s),
                    detail: format!(
                        "{reason} CPU {:.1}코어·초, 디스크 {:.1}MB를 썼습니다. 부팅 직후 바로 필요하지 않다면 시작 시 실행을 끄고 필요할 때 직접 실행하는 것을 검토하세요. {}",
                        it.cpu_seconds,
                        it.disk_mb,
                        how_to(os, it.source)
                    ),
                    item: Some(it.key.clone()),
                });
            }
        }
    }

    // R6 늦은 실행: 땡! 이후에 실행된 시작 항목
    // 예약 지연으로 일부러 늦게 실행되는 작업은 세지 않는다.
    let mut late: Vec<&ItemRecord> = rec.items.iter().filter(|i| i.late && i.launch_s.is_some() && i.delay_s < crate::judge::th::PLANNED_DELAY).collect();
    if !late.is_empty() {
        let last = late.iter().filter_map(|i| i.launch_s).fold(0.0_f64, f64::max);
        late.sort_by(|a, b| b.cpu_seconds.total_cmp(&a.cpu_seconds));
        let heavy: Vec<String> = late
            .iter()
            .take(3)
            .map(|i| format!("{}(CPU {:.1}코어·초, 디스크 {:.0}MB)", i.name, i.cpu_seconds, i.disk_mb))
            .collect();
        let mut names: Vec<&str> = rec.refreeze.iter().map(|r| r.name.as_str()).collect();
        names.dedup();
        let busy: f64 = rec.refreeze.iter().map(|r| r.to_s - r.from_s).sum();
        let refreeze = if names.is_empty() {
            String::new()
        } else {
            format!("그중 {}이(가) 실행되며 다시 얼음!이 됐습니다(합계 {busy:.0}초). ", names.join(", "))
        };
        out.push(Advice {
            rule: "R6".into(),
            title: format!("땡! 이후에도 시작프로그램 {}개가 늦게 실행됐습니다 (마지막 {last:.0}초)", late.len()),
            detail: format!(
                "{refreeze}부담이 큰 순서: {}. 시작프로그램이 차례로 실행되는 것으로 보여, 쓰지 않는 항목을 끄면 뒤에 있는 항목이 앞당겨질 수 있습니다. {}",
                heavy.join(", "),
                how_to(os, Source::RunKey)
            ),
            item: None,
        });
    }

    // R4 느려짐
    // 늦게 시작한 측정은 비교 대상에서도, 비교하는 쪽에서도 뺀다.
    let same_kind: Vec<&BootRecord> = prev.iter().filter(|p| p.boot_kind == rec.boot_kind && !p.late_start).collect();
    let recent: Vec<f64> = same_kind.iter().rev().take(5).map(|p| p.ready_s).collect();
    if let Some(m) = median(recent).filter(|_| !rec.late_start) {
        if rec.ready_s >= m * 1.2 && rec.ready_s - m >= 5.0 {
            let last = same_kind.last().unwrap();
            let new_items: Vec<&str> = rec
                .items
                .iter()
                .filter(|i| matches!(i.state, ItemStateKind::Settled | ItemStateKind::Exited | ItemStateKind::Persistent))
                .filter(|i| !last.items.iter().any(|l| l.key == i.key))
                .map(|i| i.name.as_str())
                .collect();
            let tail = if new_items.is_empty() {
                "이번에 새로 나타난 항목은 없습니다. 위 병목 항목이나 백그라운드 작업을 확인하세요.".to_string()
            } else {
                format!("이번에 새로 나타난 항목: {}.", new_items.join(", "))
            };
            out.push(Advice {
                rule: "R4".into(),
                title: format!("최근 평소보다 {:.1}초 느려졌습니다", rec.ready_s - m),
                detail: format!("최근 {}회 중앙값 {:.1}초 → 이번 {:.1}초. {tail}", same_kind.len().min(5), m, rec.ready_s),
                item: None,
            });
        }
    }

    // R2 지속형 반복, R3 미실행 반복
    let last2: Vec<&BootRecord> = prev.iter().rev().take(2).collect();
    // R3은 10분까지 지켜본 기록(v2)끼리만 센다. v1은 45초만 봐서 늦게 실행된 항목도 미실행으로 남았다.
    let last2_v2: Vec<&BootRecord> = prev.iter().rev().filter(|r| r.v >= RECORD_V).take(2).collect();
    let state_of = |r: &BootRecord, key: &str| r.items.iter().find(|i| i.key == key).map(|i| i.state);
    for it in &rec.items {
        if it.state == ItemStateKind::Persistent {
            let count = 1 + last2.iter().filter(|r| state_of(r, &it.key) == Some(ItemStateKind::Persistent)).count();
            if count >= 2 {
                out.push(Advice {
                    rule: "R2".into(),
                    title: format!("'{}'이(가) 매번 1분 넘게 작업합니다", it.name),
                    detail: format!(
                        "최근 {}회 중 {count}회 지속형이었습니다. 부팅마다 업데이트나 동기화를 하는지 프로그램 설정을 확인하세요. {}",
                        last2.len() + 1,
                        how_to(os, it.source)
                    ),
                    item: Some(it.key.clone()),
                });
            }
        }
        // 예약 지연·유휴 조건 작업은 로그온 직후 안 뜨는 게 정상이고, 파일 없음은 R7이 바로 알린다.
        let expected = it.delay_s < crate::judge::th::PLANNED_DELAY && !it.idle_only && !it.missing;
        if it.state == ItemStateKind::NotLaunched && expected && it.source != Source::Detected && rec.v >= RECORD_V && last2_v2.len() == 2 {
            let all = last2_v2.iter().all(|r| state_of(r, &it.key) == Some(ItemStateKind::NotLaunched));
            if all {
                out.push(Advice {
                    rule: "R3".into(),
                    title: format!("'{}'은(는) 등록만 되어 있고 실행되지 않습니다", it.name),
                    detail: format!(
                        "최근 3회 모두 로그온 후 10분 안에 실행되지 않았습니다. 삭제된 프로그램의 등록이 남아 있을 수 있습니다. 경로: {}. {}",
                        it.key,
                        how_to(os, it.source)
                    ),
                    item: Some(it.key.clone()),
                });
            }
        }
    }

    // R7 파일 없음: 프로그램은 삭제됐는데 시작 등록만 남은 경우. 근거가 확실해 한 번에 알린다.
    for it in rec.items.iter().filter(|i| i.missing) {
        out.push(Advice {
            rule: "R7".into(),
            title: format!("'{}'은(는) 프로그램 파일이 없는데 시작 등록만 남아 있습니다", it.name),
            detail: format!(
                "등록된 경로에 실행 파일이 없습니다: {}. 프로그램을 지우면서 등록이 남은 것으로 보입니다. {}",
                it.key,
                how_to(os, it.source)
            ),
            item: Some(it.key.clone()),
        });
    }

    // R8 체감 반응과 땡! 비교(F12): 시작프로그램이 조용해진 시점과 실제 반응이 회복된 시점이 얼마나 다른지
    if let Some(p) = rec.probe.as_ref().filter(|p| p.baseline.is_some() && !rec.late_start) {
        if let Some(r) = p.recovered_s {
            let diff = rec.ready_s - r;
            if diff >= 20.0 {
                out.push(Advice {
                    rule: "R8".into(),
                    title: format!("반응 속도는 {r:.0}초에 평소 수준으로 돌아왔습니다 (땡! {:.0}초)", rec.ready_s),
                    detail: format!(
                        "측정 중 가장 느렸을 때는 평소의 {:.1}배였습니다. 땡!보다 {diff:.0}초 일찍 써도 체감 차이는 작았을 수 있습니다. 땡!은 시작프로그램이 모두 정리된 시점이고, 반응 속도는 그보다 먼저 회복될 수 있습니다.",
                        p.peak_ratio
                    ),
                    item: None,
                });
            } else if diff <= -20.0 {
                out.push(Advice {
                    rule: "R8".into(),
                    title: format!("땡! 뒤에도 {r:.0}초까지 반응이 느렸습니다"),
                    detail: format!(
                        "측정 중 가장 느렸을 때는 평소의 {:.1}배였습니다. 시작프로그램 목록에 없는 작업(Windows 업데이트·백신 검사·인덱싱 등)이 이어졌을 수 있습니다. 상세 화면의 백그라운드 목록을 확인하세요.",
                        p.peak_ratio
                    ),
                    item: None,
                });
            }
        } else if p.peak_ratio > probe::RECOVER_RATIO {
            out.push(Advice {
                rule: "R8".into(),
                title: "측정이 끝날 때까지 반응 속도가 평소 수준으로 돌아오지 않았습니다".into(),
                detail: format!("측정 중 가장 느렸을 때는 평소의 {:.1}배였습니다. 측정 직후 잰 기준값이 평소보다 높게 잡혔을 수도 있습니다.", p.peak_ratio),
                item: None,
            });
        }
    }

    // R5 네트워크 지연
    if let Some(n) = rec.network_s {
        if n >= 20.0 {
            out.push(Advice {
                rule: "R5".into(),
                title: format!("네트워크 연결이 {n:.1}초에야 됐습니다"),
                detail: "Wi-Fi 자동 연결 설정, 무선 어댑터 드라이버, 유선이라면 DHCP 응답 지연을 확인하세요.".into(),
                item: None,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(ready: f64, items: Vec<ItemRecord>, bottleneck: Option<&str>) -> BootRecord {
        BootRecord {
            v: RECORD_V,
            os: "windows".into(),
            boot_kind: "cold".into(),
            boot_time: 0,
            anchor_time: 0,
            power_to_desktop_s: 0.0,
            event100: None,
            ready_s: ready,
            forced: false,
            grade: grade(ready, false),
            network_s: Some(3.0),
            aux_idle_s: None,
            self_cpu_avg: 0.0,
            bottleneck: bottleneck.map(String::from),
            items,
            background: vec![],
            advice: vec![],
            ..Default::default()
        }
    }

    #[test]
    fn r3_counts_only_v2_records() {
        let gone = |v: u32| {
            let mut r = rec(20.0, vec![it("x", ItemStateKind::NotLaunched, None)], None);
            r.v = v;
            r
        };
        // v1 기록 2개 + 이번 v2: 늦게 실행되던 항목일 수 있으므로 R3를 내지 않는다.
        assert!(!build(&gone(2), &[gone(1), gone(1)]).iter().any(|a| a.rule == "R3"));
        assert!(build(&gone(2), &[gone(2), gone(2)]).iter().any(|a| a.rule == "R3"));
    }

    #[test]
    fn planned_and_missing_items_are_not_r3_and_missing_is_r7() {
        let mk = |delay: f64, idle: bool, missing: bool| {
            let mut i = it("x", ItemStateKind::NotLaunched, None);
            i.delay_s = delay;
            i.idle_only = idle;
            i.missing = missing;
            rec(20.0, vec![i], None)
        };
        for r in [mk(600.0, false, false), mk(0.0, true, false)] {
            let prev = [r.clone(), r.clone()];
            assert!(!build(&r, &prev).iter().any(|a| a.rule == "R3"));
        }
        let gone = mk(0.0, false, true);
        let adv = build(&gone, &[]);
        assert!(adv.iter().any(|a| a.rule == "R7"));
        assert!(!adv.iter().any(|a| a.rule == "R3"));
    }

    #[test]
    fn r8_compares_recovery_with_ready() {
        let with = |ready: f64, rec_s: Option<f64>| {
            let mut r = rec(ready, vec![], None);
            r.probe = Some(probe::ProbeRecord {
                baseline: Some(probe::Baseline { timer_ms: 0.5, cpu_ms: 0.5, io_ms: 1.0 }),
                samples: vec![],
                recovered_s: rec_s,
                peak_ratio: 2.4,
            });
            build(&r, &[])
        };
        let early = with(100.0, Some(40.0));
        assert!(early.iter().any(|a| a.rule == "R8" && a.title.contains("40초")));
        let late = with(60.0, Some(95.0));
        assert!(late.iter().any(|a| a.rule == "R8" && a.title.contains("95초")));
        assert!(!with(60.0, Some(55.0)).iter().any(|a| a.rule == "R8"), "차이가 작으면 내지 않음");
    }

    #[test]
    fn r6_reports_late_launches_and_refreeze() {
        let mut late = it("lg", ItemStateKind::Settled, Some(160.0));
        late.late = true;
        late.launch_s = Some(153.0);
        let mut r = rec(30.0, vec![it("a", ItemStateKind::Settled, Some(20.0)), late], None);
        r.refreeze = vec![Refreeze { key: "lg".into(), name: "lg".into(), from_s: 154.0, to_s: 160.0 }];
        let adv = build(&r, &[]);
        let r6 = adv.iter().find(|a| a.rule == "R6").unwrap();
        assert!(r6.title.contains("1개") && r6.title.contains("153초"));
        assert!(r6.detail.contains("다시 얼음!") && r6.detail.contains("6초"));
    }

    #[test]
    fn late_start_is_reference_only() {
        let mut r = rec(259.1, vec![it("a", ItemStateKind::Persistent, None)], Some("a"));
        r.late_start = true;
        r.measure_start_s = 250.0;
        r.network_s = None;
        r.network_before_start = true;
        let prev: Vec<BootRecord> = (0..5).map(|_| rec(20.0, vec![], None)).collect();
        let adv = build(&r, &prev);
        let rules: Vec<&str> = adv.iter().map(|a| a.rule.as_str()).collect();
        assert_eq!(rules.first(), Some(&"R0"));
        assert!(!rules.contains(&"R1") && !rules.contains(&"R4") && !rules.contains(&"R5"));
    }

    fn it(key: &str, state: ItemStateKind, settle: Option<f64>) -> ItemRecord {
        ItemRecord {
            key: key.into(),
            name: key.into(),
            source: Source::RunKey,
            state,
            launch_s: Some(2.0),
            settle_s: settle,
            peak_cpu: 50.0,
            cpu_seconds: 3.0,
            disk_mb: 40.0,
            late: false,
            delay_s: 0.0,
            idle_only: false,
            missing: false,
        }
    }

    #[test]
    fn grades() {
        assert_eq!(grade(30.0, false), "A");
        assert_eq!(grade(30.1, false), "B");
        assert_eq!(grade(60.0, false), "B");
        assert_eq!(grade(120.0, false), "C");
        assert_eq!(grade(121.0, false), "D");
        assert_eq!(grade(10.0, true), "D");
    }

    #[test]
    fn r1_only_when_late() {
        let r = rec(12.0, vec![it("a", ItemStateKind::Settled, Some(12.0))], Some("a"));
        assert!(build(&r, &[]).is_empty());
        let r = rec(25.0, vec![it("a", ItemStateKind::Settled, Some(25.0))], Some("a"));
        assert_eq!(build(&r, &[])[0].rule, "R1");
    }

    #[test]
    fn r4_names_new_items() {
        let prev: Vec<BootRecord> =
            (0..5).map(|_| rec(20.0, vec![it("a", ItemStateKind::Settled, Some(20.0))], None)).collect();
        let r = rec(
            30.0,
            vec![it("a", ItemStateKind::Settled, Some(20.0)), it("b", ItemStateKind::Settled, Some(30.0))],
            None,
        );
        let adv = build(&r, &prev);
        let r4 = adv.iter().find(|a| a.rule == "R4").unwrap();
        assert!(r4.detail.contains("b"));
    }

    #[test]
    fn r2_and_r3_need_repetition() {
        let p = rec(20.0, vec![it("a", ItemStateKind::Persistent, None), it("z", ItemStateKind::NotLaunched, None)], None);
        let r = rec(20.0, vec![it("a", ItemStateKind::Persistent, None), it("z", ItemStateKind::NotLaunched, None)], None);
        let adv = build(&r, &[p.clone()]);
        assert!(adv.iter().any(|a| a.rule == "R2"));
        assert!(!adv.iter().any(|a| a.rule == "R3"));
        let adv = build(&r, &[p.clone(), p]);
        assert!(adv.iter().any(|a| a.rule == "R3"));
    }
}
