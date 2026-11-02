const $ = (id) => document.getElementById(id);
const SVG = "http://www.w3.org/2000/svg";
const METHOD = { task_scheduler: "작업 스케줄러", run_key: "Run 키(지연 실행)", launch_agent: "LaunchAgent", none: "" };
const BAR_COLOR = {
  settled: "var(--ok)",
  exited: "var(--ok)",
  persistent: "var(--run)",
  not_launched: "var(--off)",
  untrackable: "var(--off)",
};

let data = { records: [], demo: null };
let selected = null;

function svg(tag, attrs = {}, text) {
  const n = document.createElementNS(SVG, tag);
  for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, v);
  if (text !== undefined) n.textContent = text;
  return n;
}

function when(epoch) {
  const d = new Date(epoch * 1000);
  const p = (x) => String(x).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

function stat(label, value, sub, help) {
  return el(
    "div",
    { class: "stat" },
    el("div", { class: "label" }, label, help ? tip(help) : null),
    el("div", { class: "value" }, value, sub ? el("small", {}, ` ${sub}`) : null),
  );
}

function renderCards(r) {
  const cards = [
    stat(
      "사용 가능까지",
      el("span", {}, secs(r.ready_s), " ", gradeBadge(r)),
      r.forced ? "강제 판정" : r.late_start ? `측정 ${Math.round(r.measure_start_s)}초 늦게 시작` : "",
      "바탕화면이 뜬 시점부터 필수 체크리스트가 모두 충족될 때까지. 등급 A ≤30초, B ≤60초, C ≤120초, D 그 이상.",
    ),
    stat(
      "부팅 종류",
      bootKindLabel(r),
      "",
      r.boot_source === "event"
        ? "Windows 시스템 로그의 부팅 유형(Kernel-Boot 이벤트 27)입니다."
        : "시스템 로그를 읽지 못해, 부팅 시각과 바탕화면 시각이 10분 넘게 떨어져 있으면 빠른 시작으로 추정했습니다. 로그인 화면에서 오래 기다린 콜드 부팅도 빠른 시작으로 보일 수 있습니다.",
    ),
  ];
  if (r.power_to_logon_s != null && r.logon_to_desktop_s != null) {
    cards.push(
      stat("부팅 → 로그온", secs(r.power_to_logon_s), "로그인 대기 포함", "부팅 유형 이벤트 시각부터 로그온(Winlogon 7001)까지. 로그인 화면에서 기다린 시간이 들어갑니다."),
      stat("로그온 → 바탕화면", secs(r.logon_to_desktop_s), "", "로그온부터 탐색기(바탕화면) 시작까지."),
    );
  } else if (r.boot_kind !== "demo") {
    cards.push(
      stat(
        "전원 → 바탕화면",
        secs(r.power_to_desktop_s),
        r.os === "windows" ? "로그인 대기 포함" : "",
        "OS 부팅 시각부터 바탕화면 표시까지. Windows는 비밀번호 입력 대기 시간이 포함됩니다.",
      ),
    );
  }
  if (r.event100) {
    cards.push(
      stat(
        "Event 100",
        `${(r.event100.boot_ms / 1000).toFixed(1)}초`,
        `메인 ${(r.event100.main_path_ms / 1000).toFixed(1)} + 후속 ${(r.event100.post_boot_ms / 1000).toFixed(1)}`,
        "Windows 부팅 성능 이벤트. 관리자 권한이 있을 때만 읽힙니다.",
      ),
    );
  }
  const late = r.items.filter((i) => i.late);
  if (late.length) {
    const busy = (r.refreeze || []).reduce((s, f) => s + (f.to_s - f.from_s), 0);
    cards.push(
      stat(
        "시작프로그램 실행 완료",
        secs(r.all_launched_s),
        `늦게 실행 ${late.length}개${busy > 0 ? ` · 다시 얼음! ${Math.round(busy)}초` : ""}`,
        "마지막 시작프로그램이 실행된 시각. 45초 이후 실행된 항목은 땡! 판정을 막지 않고, CPU를 많이 쓰면 그동안 다시 얼음!으로 표시했습니다.",
      ),
    );
  }
  cards.push(
    stat(
      "네트워크",
      r.network_s != null ? secs(r.network_s) : r.network_before_start ? "측정 전 연결" : "연결 안 됨",
      "",
      r.network_before_start ? "측정을 시작했을 때 이미 연결돼 있어 연결된 시각은 알 수 없습니다." : "",
    ),
    stat("시스템 안정(보조)", r.aux_idle_s !== null ? secs(r.aux_idle_s) : "-", "", "시스템 CPU 사용률 20% 이하 구간이 누적 10초가 된 시점. Microsoft Post Boot 정의의 근사치이며 판정에는 쓰지 않습니다."),
    stat("땡해줘 자체 CPU", `${r.self_cpu_avg.toFixed(1)}%`, "평균", "측정 중 땡해줘 자신이 쓴 CPU(코어 1개 기준). 측정 부하 확인용입니다."),
  );
  $("cards").replaceChildren(...cards);
}

function renderTimeline(r) {
  const items = r.items.filter((i) => i.state !== "untrackable");
  // 지속형 막대는 실행 + 60초까지(그때 지속형으로 분류됨).
  const endOf = (i) => i.settle_s ?? (i.state === "persistent" && i.launch_s != null ? Math.max(i.launch_s, 0) + 60 : r.ready_s);
  const ends = items.filter((i) => i.state !== "not_launched").map(endOf);
  const starts = items.map((i) => i.launch_s ?? 0);
  const minX = Math.min(0, ...starts.filter((s) => s > -30), 0);
  const maxX = Math.max(r.ready_s, r.network_s ?? 0, ...ends, ...starts, 10) * 1.08;
  const W = Math.max(560, $("timeline").clientWidth || 760);
  const L = 170;
  const R = 16;
  const rowH = 22;
  const top = 22;
  const H = top + Math.max(items.length, 1) * rowH + 8;
  const x = (s) => L + ((Math.max(s, minX) - minX) / (maxX - minX)) * (W - L - R);
  const g = svg("svg", { width: W, height: H, viewBox: `0 0 ${W} ${H}`, role: "img", "aria-label": "시작 항목 타임라인" });

  const step = maxX > 120 ? 30 : maxX > 60 ? 10 : 5;
  for (let s = Math.ceil(minX / step) * step; s <= maxX; s += step) {
    g.append(svg("line", { x1: x(s), x2: x(s), y1: top - 4, y2: H - 4, class: "grid" }));
    g.append(svg("text", { x: x(s), y: 12, "text-anchor": "middle", class: "axis" }, `${s}s`));
  }

  for (const f of r.refreeze || []) {
    const band = svg("rect", { x: x(f.from_s), y: top - 4, width: Math.max(2, x(f.to_s) - x(f.from_s)), height: H - top, style: "fill:var(--wait-soft)" });
    band.append(svg("title", {}, `다시 얼음! ${secs(f.from_s)} ~ ${secs(f.to_s)} · ${f.name}`));
    g.append(band);
  }

  items.forEach((i, idx) => {
    const y = top + idx * rowH;
    const label = i.name.length > 22 ? `${i.name.slice(0, 21)}…` : i.name;
    const t = svg("text", { x: 4, y: y + 15, class: "row-label" }, label);
    t.append(svg("title", {}, `${i.name} · ${SOURCE_LABEL[i.source]} · ${STATE_LABEL[i.state]}`));
    g.append(t);
    if (i.state === "not_launched") {
      g.append(svg("text", { x: x(0) + 4, y: y + 15, class: "axis" }, "미실행"));
      return;
    }
    const a = i.launch_s ?? 0;
    const b = endOf(i);
    const bar = svg("rect", {
      x: x(a),
      y: y + 5,
      width: Math.max(3, x(b) - x(a)),
      height: 12,
      rx: 3,
      style: `fill:${BAR_COLOR[i.state] || "var(--wait)"};opacity:${i.key === r.bottleneck ? 1 : 0.6}`,
    });
    bar.append(
      svg(
        "title",
        {},
        `${i.name}${i.late ? " (늦은 실행)" : ""}\n실행 ${secs(i.launch_s)} → ${i.state === "persistent" ? "지속형" : `안정 ${secs(i.settle_s)}`}\n최대 CPU ${Math.round(i.peak_cpu)}% · CPU ${i.cpu_seconds.toFixed(1)}코어·초 · 디스크 ${i.disk_mb.toFixed(1)}MB`,
      ),
    );
    g.append(bar);
  });

  if (r.network_s !== null) {
    g.append(svg("line", { x1: x(r.network_s), x2: x(r.network_s), y1: top - 4, y2: H - 4, class: "net" }));
  }
  g.append(svg("line", { x1: x(r.ready_s), x2: x(r.ready_s), y1: top - 4, y2: H - 4, class: "ready" }));

  $("timeline").replaceChildren(g);
  $("legend").replaceChildren(
    el("span", {}, el("i", { style: "background:var(--ok)" }), "실행→안정"),
    el("span", {}, el("i", { style: "background:var(--run)" }), "지속형"),
    el("span", {}, el("i", { style: "background:var(--ok);width:2px" }), "사용 가능"),
    el("span", {}, el("i", { style: "background:var(--run);width:2px" }), "네트워크"),
    ...(r.refreeze && r.refreeze.length ? [el("span", {}, el("i", { style: "background:var(--wait-soft)" }), "다시 얼음!")] : []),
  );
}

function renderAdvice(r) {
  const box = $("advice");
  if (!r.advice.length) {
    box.replaceChildren(el("p", { class: "muted" }, "이번 측정에서는 근거가 충분한 추천이 없습니다."));
    return;
  }
  box.replaceChildren(
    ...r.advice.map((a) =>
      el("div", { class: "adv" }, el("b", {}, el("span", { class: "rule" }, a.rule), a.title), el("span", {}, a.detail)),
    ),
  );
}

function renderRecord(r) {
  $("current").hidden = !r;
  $("empty").hidden = !!r || data.records.length > 0;
  if (!r) return;
  $("recTitle").textContent = r.boot_kind === "demo" ? "데모 측정 결과" : "부팅 측정 결과";
  $("recWhen").textContent = when(r.anchor_time);
  renderCards(r);
  renderTimeline(r);
  renderAdvice(r);
}

function renderHistory() {
  const rows = data.records
    .slice()
    .reverse()
    .slice(0, 30)
    .map((r) => {
      const bn = r.items.find((i) => i.key === r.bottleneck);
      const tr = el(
        "tr",
        { class: r === selected ? "sel" : "" },
        el("td", { class: "num" }, when(r.anchor_time)),
        el("td", {}, bootKindLabel(r)),
        el("td", { class: "num" }, secs(r.ready_s)),
        el("td", {}, gradeBadge(r)),
        el("td", {}, bn ? bn.name : "-"),
        el("td", { class: "num" }, r.items.length),
      );
      tr.addEventListener("click", () => {
        selected = r;
        renderRecord(r);
        renderHistory();
      });
      return tr;
    });
  $("history").querySelector("tbody").replaceChildren(
    ...(rows.length ? rows : [el("tr", {}, el("td", { colspan: 6, class: "muted" }, "기록 없음"))]),
  );
}

async function load() {
  data = await invoke("get_history");
  const last = data.records[data.records.length - 1] || null;
  const demoNewer = data.demo && (!last || data.demo.anchor_time > last.anchor_time);
  selected = demoNewer ? data.demo : last;
  renderRecord(selected);
  renderHistory();
}

async function loadAuto() {
  const st = await invoke("get_autostart");
  $("auto").checked = st.enabled;
  $("autoMethod").textContent = st.enabled && METHOD[st.method] ? `(${METHOD[st.method]})` : "";
  $("autoWrap").title =
    st.method === "run_key"
      ? "작업 스케줄러 등록에 실패해 Run 키로 등록했습니다. Run 키 항목은 Windows가 로그인 후 늦게 실행해서 측정 초반 몇 초를 놓칠 수 있습니다."
      : "로그인할 때 땡해줘를 자동으로 실행합니다.";
}

function renderUpdate(u) {
  if (!u) return;
  $("ver").textContent = `v${u.current}`;
  $("ver").title = u.last_check ? `마지막 업데이트 확인: ${when(u.last_check)}` : "아직 업데이트를 확인하지 않았습니다.";
  const box = $("upd");
  const btn = $("updInstall");
  $("updCheck").disabled = u.checking || u.installing || u.measuring;
  $("updCheck").textContent = u.checking ? "확인 중…" : "업데이트 확인";
  $("updCheck").title = u.measuring ? "측정 중에는 확인하지 않습니다(측정 결과에 영향)." : "새 버전이 있는지 지금 확인합니다. 자동 확인은 측정이 끝난 뒤 6시간에 한 번입니다.";
  box.classList.toggle("err", !!u.error && !u.available);
  if (u.installing) {
    box.hidden = false;
    $("updTitle").textContent = `새 버전 ${u.available?.version ?? ""} 설치 중…`;
    $("updSub").textContent = "내려받은 뒤 설치 창이 잠깐 뜨고, 끝나면 다시 실행됩니다.";
    btn.hidden = true;
  } else if (u.available) {
    box.hidden = false;
    $("updTitle").textContent = `새 버전 ${u.available.version}이 있습니다 (현재 ${u.current})`;
    $("updSub").textContent = [u.error ? `설치 실패: ${u.error}` : "", u.available.notes || ""].filter(Boolean).join("\n");
    btn.hidden = false;
    btn.disabled = u.measuring;
    btn.title = u.measuring ? "측정이 끝나면 설치할 수 있습니다." : "내려받아 설치하고 다시 실행합니다. 자동 시작 설정과 부팅 이력은 유지됩니다.";
  } else if (u.error) {
    box.hidden = false;
    $("updTitle").textContent = "업데이트 확인에 실패했습니다";
    $("updSub").textContent = u.error;
    btn.hidden = true;
  } else {
    box.hidden = true;
  }
}

$("updCheck").addEventListener("click", () => invoke("check_update", { force: true }));
$("updInstall").addEventListener("click", () => invoke("install_update").catch((e) => renderUpdate({ ...lastUpd, error: String(e) })));
let lastUpd = null;
listen("update-state", (u) => { lastUpd = u; renderUpdate(u); });
// 창을 볼 때 자동 확인(6시간 간격 제한은 앱이 판단)
window.addEventListener("focus", () => invoke("check_update", { force: false }));

$("auto").addEventListener("change", async (e) => {
  await invoke("set_autostart", { enabled: e.target.checked });
  loadAuto();
});
$("demo").addEventListener("click", () => invoke("start_demo"));
listen("history-updated", load);
window.addEventListener("resize", () => selected && renderTimeline(selected));

load();
loadAuto();
invoke("get_update").then((u) => { lastUpd = u; renderUpdate(u); });
invoke("check_update", { force: false });
