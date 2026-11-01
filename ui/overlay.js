const $ = (id) => document.getElementById(id);
const HIDE_AFTER_MS = 15000;
let hideTimer = null;
let hovering = false;
let lastPhase = null;

function scheduleHide() {
  clearTimeout(hideTimer);
  if (hovering || lastPhase !== "done") return;
  hideTimer = setTimeout(() => invoke("hide_overlay"), HIDE_AFTER_MS);
}

function render(v) {
  if (!v) return;
  const ready = v.ready_s !== null;
  $("dot").className = `dot ${ready ? "ready" : "busy"}`;
  $("title").textContent = ready ? (v.forced ? "사용 가능 (일부 지연)" : "사용 가능") : "준비 중";
  $("clock").textContent = clock(v.t);
  $("demo").hidden = !v.demo;

  const { done, total } = v.progress;
  const pct = ready ? 100 : total === 0 ? 50 : Math.round((done / total) * 100);
  $("fill").style.width = `${pct}%`;
  $("fill").className = ready ? "ready" : "";

  const sub = $("sub");
  sub.replaceChildren();
  if (ready) {
    sub.append(`바탕화면 후 ${secs(v.ready_s)} · 등급 `, el("span", { class: `grade ${v.grade}` }, v.grade));
  } else {
    sub.append(`시작 항목 ${done} / ${total} 정리됨`);
  }

  const c = v.checklist;
  const mark = (s) => (s === "ok" ? "✔" : s === "skip" ? "–" : "◌");
  $("checks").replaceChildren(
    el("li", { class: c.desktop }, `${mark(c.desktop)} 바탕화면`),
    el("li", { class: c.items }, `${mark(c.items)} 시작 항목`),
    el("li", { class: c.network, title: v.network_note }, `${mark(c.network)} 네트워크`),
  );

  const rows = v.items.map((i) =>
    el(
      "li",
      { title: `${SOURCE_LABEL[i.source] || i.source} · ${i.key}` },
      el("span", { class: "name" }, i.name),
      el("span", { class: "cpu num muted" }, i.state === "running" || i.state === "persistent" ? `${Math.round(i.cpu)}%` : ""),
      stateBadge(i.state),
      el("span", { class: "time num muted" }, i.settle_s !== null ? secs(i.settle_s) : ""),
    ),
  );
  $("items").replaceChildren(...(rows.length ? rows : [el("li", { class: "empty" }, "추적 중인 시작 항목이 없습니다.")]));

  const bg = $("bg");
  bg.replaceChildren();
  if (v.background.length) {
    bg.append("백그라운드: ", v.background.map((b) => `${b.name} ${Math.round(b.cpu)}%`).join(", "), " ");
    bg.append(tip("판정을 막지 않는 작업입니다. 끝나지 않을 수 있어 참고로만 표시합니다."));
  }

  const adv = $("advice");
  if (v.top_advice) {
    adv.hidden = false;
    adv.replaceChildren(el("b", {}, v.top_advice.title), v.top_advice.detail);
  } else {
    adv.hidden = true;
  }

  if (v.phase !== lastPhase) {
    lastPhase = v.phase;
    scheduleHide();
  }
}

$("card").addEventListener("mouseenter", () => {
  hovering = true;
  clearTimeout(hideTimer);
});
$("card").addEventListener("mouseleave", () => {
  hovering = false;
  scheduleHide();
});
$("close").addEventListener("click", () => invoke("hide_overlay"));
$("more").addEventListener("click", () => invoke("open_details"));

listen("session", (v) => {
  if (v.phase === "preparing" && lastPhase === "done") lastPhase = null;
  render(v);
});
invoke("get_view").then(render);
