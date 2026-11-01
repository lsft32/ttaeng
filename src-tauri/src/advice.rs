//! 평가 등급과 추천. 측정값이 근거인 규칙만 둔다.

use crate::model::*;

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

    // R1 병목
    if let Some(key) = &rec.bottleneck {
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

    // R4 느려짐
    let same_kind: Vec<&BootRecord> = prev.iter().filter(|p| p.boot_kind == rec.boot_kind).collect();
    let recent: Vec<f64> = same_kind.iter().rev().take(5).map(|p| p.ready_s).collect();
    if let Some(m) = median(recent) {
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
        if it.state == ItemStateKind::NotLaunched && it.source != Source::Detected && last2.len() == 2 {
            let all = last2.iter().all(|r| state_of(r, &it.key) == Some(ItemStateKind::NotLaunched));
            if all {
                out.push(Advice {
                    rule: "R3".into(),
                    title: format!("'{}'은(는) 등록만 되어 있고 실행되지 않습니다", it.name),
                    detail: format!(
                        "최근 3회 모두 실행되지 않았습니다. 삭제된 프로그램의 등록이 남아 있을 수 있습니다. 경로: {}. {}",
                        it.key,
                        how_to(os, it.source)
                    ),
                    item: Some(it.key.clone()),
                });
            }
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
            v: 1,
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
        }
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
