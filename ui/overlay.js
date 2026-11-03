const $ = (id) => document.getElementById(id);
const HIDE_AFTER_MS = 15000;
let hideTimer = null;
let hovering = false;
let lastPhase = null;

let lastRefrozen = false;

function scheduleHide() {
  clearTimeout(hideTimer);
  // 땡! 뒤 늦은 항목을 지켜보는 동안에도 숨긴다. 다시 얼음!이 되면 앱이 창을 다시 띄운다.
  if (hovering || lastRefrozen || (lastPhase !== "watching" && lastPhase !== "done")) return;
  hideTimer = setTimeout(() => invoke("hide_overlay"), HIDE_AFTER_MS);
}

function render(v) {
  if (!v) return;
  const ready = v.ready_s !== null && !v.refrozen;
  $("dot").className = `dot ${ready ? "ready" : "busy"}`;
  const title = $("title");
  const word = ready ? "땡!" : "얼음!";
  if (title.textContent !== word) {
    title.textContent = word;
    // 클래스를 뺐다 넣어야 애니메이션이 다시 돈다.
    title.className = "";
    void title.offsetWidth;
    title.className = ready ? "thaw" : "freeze";
  }
  $("forced").hidden = !(ready && v.forced);
  const declared = v.ready_s !== null;
  $("clock").textContent = clock(v.t);
  $("demo").hidden = !v.demo;

  const { done, total } = v.progress;
  const pct = ready ? 100 : total === 0 ? 50 : Math.round((done / total) * 100);
  $("fill").style.width = `${pct}%`;
  $("fill").className = ready ? "ready" : "";

  const sub = $("sub");
  sub.replaceChildren();
  if (declared) {
    sub.append(`바탕화면 후 ${secs(v.ready_s)} · 등급 `, el("span", { class: `grade ${v.grade}` }, v.grade));
    if (v.refrozen) {
      sub.append(" · ", el("span", { class: "late", title: "땡! 이후 늦게 실행된 시작프로그램이 CPU를 많이 쓰고 있습니다. 조용해지면 다시 땡!이 됩니다." }, "늦게 실행된 항목 처리 중"));
    } else if (v.deferred > 0) {
      sub.append(" · ", el("span", { title: "45초 안에 실행되지 않은 항목입니다. 판정은 막지 않고 최대 10분까지 지켜봅니다." }, `늦게 실행 대기 ${v.deferred}개`));
    }
  } else {
    sub.append(`시작 항목 ${done} / ${total} 정리됨`);
  }
  if (!v.demo && v.measure_start_s >= 30) {
    sub.append(" · ", el("span", { class: "late", title: "그 전에 끝난 작업의 정확한 시점은 알 수 없어 결과는 참고용입니다." }, `측정 ${Math.round(v.measure_start_s)}초 늦게 시작`));
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
      (() => {
        const n = i.settle_s === null ? plannedNote(i) : null;
        return el("span", { class: "time num muted", title: n ? n.tip : "" }, i.settle_s !== null ? secs(i.settle_s) : n ? n.text : "");
      })(),
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

  if (v.phase !== lastPhase || v.refrozen !== lastRefrozen) {
    lastPhase = v.phase;
    lastRefrozen = v.refrozen;
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
