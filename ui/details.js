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
      el("span", {}, secs(r.ready_s), " ", el("span", { class: `grade ${r.grade}` }, r.grade)),
      r.forced ? "강제 판정" : "",
      "바탕화면이 뜬 시점부터 필수 체크리스트가 모두 충족될 때까지. 등급 A ≤30초, B ≤60초, C ≤120초, D 그 이상.",
    ),
    stat("부팅 종류", BOOT_KIND[r.boot_kind] || r.boot_kind, "", "Windows에서 부팅 시각과 바탕화면 시각이 10분 넘게 떨어져 있으면 빠른 시작 또는 재로그인으로 추정합니다."),
  ];
  if (r.boot_kind !== "demo") {
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
  cards.push(
    stat("네트워크", r.network_s !== null ? secs(r.network_s) : "연결 안 됨"),
    stat("시스템 안정(보조)", r.aux_idle_s !== null ? secs(r.aux_idle_s) : "-", "", "시스템 CPU 사용률 20% 이하 구간이 누적 10초가 된 시점. Microsoft Post Boot 정의의 근사치이며 판정에는 쓰지 않습니다."),
    stat("땡해줘 자체 CPU", `${r.self_cpu_avg.toFixed(1)}%`, "평균", "측정 중 땡해줘 자신이 쓴 CPU(코어 1개 기준). 측정 부하 확인용입니다."),
  );
  $("cards").replaceChildren(...cards);
}

function renderTimeline(r) {
  const items = r.items.filter((i) => i.state !== "untrackable");
  const ends = items.map((i) => i.settle_s ?? 0);
  const starts = items.map((i) => i.launch_s ?? 0);
  const minX = Math.min(0, ...starts.filter((s) => s > -30), 0);
  const maxX = Math.max(r.ready_s, r.network_s ?? 0, ...ends, 10) * 1.08;
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
    const b = i.settle_s ?? r.ready_s;
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
        `${i.name}\n실행 ${secs(i.launch_s)} → ${i.state === "persistent" ? "지속형" : `안정 ${secs(i.settle_s)}`}\n최대 CPU ${Math.round(i.peak_cpu)}% · CPU ${i.cpu_seconds.toFixed(1)}코어·초 · 디스크 ${i.disk_mb.toFixed(1)}MB`,
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
        el("td", {}, BOOT_KIND[r.boot_kind] || r.boot_kind),
        el("td", { class: "num" }, secs(r.ready_s)),
        el("td", {}, el("span", { class: `grade ${r.grade}` }, r.grade)),
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
