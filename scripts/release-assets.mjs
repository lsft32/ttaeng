// 릴리스 파일 정리와 latest.json 생성.
// GitHub는 릴리스 파일 이름의 한글을 지우므로(예: "땡해줘_0.3.0_x64-setup.exe" → "_0.3.0_x64-setup.exe")
// 업로드 전에 ASCII 이름(ttaeng_...)으로 바꾸고, 업데이터가 읽는 latest.json도 직접 만든다.
//
//   node scripts/release-assets.mjs collect <bundle 폴더> <출력 폴더>
//   node scripts/release-assets.mjs latest <파일 폴더> <버전> <릴리스 다운로드 URL 접두> [노트]

import { copyFileSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";

const PRODUCT = "땡해줘";
const ASCII = "ttaeng";

/** 업데이트·설치에 쓰는 파일만 고른다. */
const WANTED = [/\.exe$/, /\.exe\.sig$/, /\.dmg$/, /\.app\.tar\.gz$/, /\.app\.tar\.gz\.sig$/];

export function asciiName(name) {
  return name.startsWith(PRODUCT) ? ASCII + name.slice(PRODUCT.length) : name;
}

function walk(dir, out = []) {
  for (const f of readdirSync(dir)) {
    const p = join(dir, f);
    // .app 번들 폴더 안은 볼 필요 없다(압축본 .app.tar.gz를 쓴다).
    if (statSync(p).isDirectory()) {
      if (!f.endsWith(".app")) walk(p, out);
    } else out.push(p);
  }
  return out;
}

function collect(bundleDir, outDir) {
  mkdirSync(outDir, { recursive: true });
  const files = walk(bundleDir).filter((p) => WANTED.some((re) => re.test(p)));
  if (!files.length) throw new Error(`릴리스 파일이 없습니다: ${bundleDir}`);
  for (const p of files) {
    const to = join(outDir, asciiName(basename(p)));
    copyFileSync(p, to);
    console.log(`${basename(p)} -> ${basename(to)}`);
  }
}

/** Tauri 업데이터 형식. 플랫폼 키별로 서명(.sig 내용)과 내려받을 URL. */
export function buildLatest(names, readSig, version, urlBase, notes) {
  const find = (re) => names.find((n) => re.test(n) && !n.endsWith(".sig"));
  const entry = (file) => {
    if (!file) return null;
    const sig = names.find((n) => n === `${file}.sig`);
    if (!sig) throw new Error(`서명 파일이 없습니다: ${file}.sig`);
    return { signature: readSig(sig).trim(), url: `${urlBase.replace(/\/$/, "")}/${encodeURIComponent(file)}` };
  };
  const platforms = {};
  const win = entry(names.find((n) => n.endsWith(`_${version}_x64-setup.exe`)));
  if (win) platforms["windows-x86_64"] = win;
  const mac = entry(find(/\.app\.tar\.gz$/));
  if (mac) {
    // universal 빌드 하나로 두 아키텍처를 받는다.
    platforms["darwin-aarch64"] = mac;
    platforms["darwin-x86_64"] = mac;
  }
  if (!Object.keys(platforms).length) throw new Error("업데이트 파일이 하나도 없습니다.");
  return { version, notes, pub_date: new Date().toISOString(), platforms };
}

function latest(dir, version, urlBase, notes) {
  const names = readdirSync(dir);
  const json = buildLatest(names, (n) => readFileSync(join(dir, n), "utf8"), version.replace(/^v/, ""), urlBase, notes || `땡해줘 ${version}`);
  const out = join(dir, "latest.json");
  writeFileSync(out, JSON.stringify(json, null, 2) + "\n");
  console.log(`latest.json: ${Object.keys(json.platforms).join(", ")}`);
}

const [cmd, ...args] = process.argv.slice(2);
if (cmd === "collect" && args.length === 2) collect(...args);
else if (cmd === "latest" && args.length >= 3) latest(...args);
else if (cmd !== undefined) {
  console.error("usage: collect <bundleDir> <outDir> | latest <dir> <version> <urlBase> [notes]");
  process.exit(2);
}
