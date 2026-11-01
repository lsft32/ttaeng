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
  running: "작업 중",
  settled: "안정",
  exited: "완료",
  persistent: "지속형",
  not_launched: "미실행",
  untrackable: "추적 불가",
};

const STATE_TIP = {
  waiting: "등록돼 있지만 아직 실행되지 않았습니다. 45초까지 기다립니다.",
  running: "실행 중이며 아직 조용해지지 않았습니다.",
  settled: "CPU 3% 미만·디스크 1MB/s 미만·새 자식 프로세스 없음이 5초 연속 유지됐습니다.",
  exited: "실행 후 스스로 종료했습니다(런처 등).",
  persistent: "실행 후 60초가 지나도 계속 작업해 판정에서 제외했습니다.",
  not_launched: "45초 안에 실행되지 않아 판정에서 제외했습니다.",
  untrackable: "스크립트·호스트 실행이라 프로세스를 특정할 수 없어 제외했습니다.",
};

const SOURCE_LABEL = {
  run_key: "Run 키",
  startup_folder: "시작 폴더",
  logon_task: "예약 작업",
  launch_agent: "LaunchAgent",
  detected: "자동 감지",
};

const BOOT_KIND = { cold: "콜드 부팅", fast_startup: "빠른 시작·재로그인(추정)", demo: "데모 측정" };

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
