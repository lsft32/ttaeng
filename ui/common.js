// 공통 헬퍼. 이름은 레지스트리 등 외부 값이라 항상 textContent로 넣는다.
const T = window.__TAURI__;
const invoke = (cmd, args) => T.core.invoke(cmd, args);
const listen = (ev, fn) => T.event.listen(ev, (e) => fn(e.payload));

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === undefined || v === null) continue;
    if (k === "class") node.className = v;
    else if (k.startsWith("on")) node.addEventListener(k.slice(2), v);
    else node.setAttribute(k, v);
  }
  for (const c of children.flat()) {
    if (c === null || c === undefined || c === false) continue;
    node.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return node;
}

const STATE_LABEL = {
  waiting: "대기",
  deferred: "늦게 실행 대기",
  running: "작업 중",
  settled: "안정",
  exited: "완료",
  persistent: "지속형",
  not_launched: "미실행",
  untrackable: "추적 불가",
};

const STATE_TIP = {
  waiting: "등록돼 있지만 아직 실행되지 않았습니다. 45초까지 기다리고, 그 뒤에는 '늦게 실행 대기'로 바꿔 판정을 막지 않습니다.",
  deferred: "45초 안에 실행되지 않았습니다. Windows가 시작프로그램을 차례로 띄우는 중일 수 있어 땡! 이후에도 최대 10분까지 지켜봅니다. 판정은 막지 않습니다.",
  running: "실행 중이며 아직 조용해지지 않았습니다.",
  settled: "최근 5초 평균 CPU 8% 미만(코어 1개 기준)·디스크 1MB/s 미만·새 자식 프로세스 없음을 만족했습니다.",
  exited: "실행 후 스스로 종료했습니다(런처 등).",
  persistent: "실행 후 60초가 지나도 계속 작업해 판정에서 제외했습니다.",
  not_launched: "로그온 후 10분 안에 실행되지 않았습니다(이전 버전 기록은 45초 기준).",
  untrackable: "스크립트·호스트 실행이라 프로세스를 특정할 수 없어 제외했습니다.",
};

const SOURCE_LABEL = {
  run_key: "Run 키",
  startup_folder: "시작 폴더",
  logon_task: "예약 작업",
  launch_agent: "LaunchAgent",
  detected: "자동 감지",
};

const BOOT_KIND = { cold: "콜드 부팅", fast_startup: "빠른 시작", resume: "최대 절전 복귀", demo: "데모 측정" };

// v1.3 이전 기록은 boot_source가 없고 빠른 시작을 10분 규칙으로 추정했다.
function bootKindLabel(r) {
  const label = BOOT_KIND[r.boot_kind] || r.boot_kind;
  return r.boot_source === "event" || r.boot_kind === "cold" || r.boot_kind === "demo" ? label : `${label}(추정)`;
}

// 측정이 늦게 시작된 기록의 등급은 참고용이다.
function gradeBadge(r) {
  return el(
    "span",
    { class: `grade ${r.grade}${r.late_start ? " ref" : ""}`, title: r.late_start ? `측정이 바탕화면보다 ${Math.round(r.measure_start_s)}초 늦게 시작돼 참고용입니다.` : "" },
    r.late_start ? `${r.grade} 참고` : r.grade,
  );
}

function stateBadge(state) {
  return el("span", { class: `state ${state}`, title: STATE_TIP[state] || "" }, STATE_LABEL[state] || state);
}

function secs(v) {
  if (v === null || v === undefined) return "-";
  return `${v.toFixed(1)}초`;
}

function clock(t) {
  const s = Math.max(0, Math.floor(t));
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}

function tip(text) {
  return el("span", { class: "tip", title: text }, "i");
}
