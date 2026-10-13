// Asserts every download the site advertises actually resolves right now.
//
// This is the guard that was missing. check-downloads.mjs only proves a
// filename appears in release.yml — that the *workflow intends* to upload it.
// It says nothing about whether a release containing it has been published.
//
// That gap shipped a real failure: arm64 downloads were added to the site and
// deployed on merge, but the tag that publishes those assets had not been
// pushed, so every ARM64 button 404'd for whoever clicked it first.
//
// Running this in the deploy job enforces the rollout order the spec requires
// — tag the release, then deploy the site — by refusing to publish a page
// that promises files nobody can download.
//
// Skips itself when offline or when GitHub is unreachable: a network blip
// should not block a deploy that is otherwise correct. Only a definitive 404
// fails.
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const siteSrc = readFileSync(resolve(here, "../src/lib/site.ts"), "utf8");

const baseMatch = siteSrc.match(/RELEASES_BASE\s*=\s*\n?\s*"([^"]+)"/);
if (!baseMatch) {
  console.error("check-live-downloads: could not read RELEASES_BASE");
  process.exit(1);
}
const base = baseMatch[1];

const start = siteSrc.indexOf("export const DOWNLOADS");
const end = siteSrc.indexOf("\n];", start);
if (start === -1 || end === -1) {
  console.error("check-live-downloads: could not read DOWNLOADS");
  process.exit(1);
}
const files = [
  ...siteSrc.slice(start, end).matchAll(/file:\s*"([^"]+)"/g),
].map((m) => m[1]);
files.push("SHA256SUMS");

async function head(url) {
  // Follows the releases/latest redirect to the asset itself.
  for (let attempt = 1; attempt <= 3; attempt++) {
    try {
      const res = await fetch(url, { method: "GET", redirect: "follow" });
      return res.status;
    } catch (err) {
      if (attempt === 3) return { offline: String(err) };
      await new Promise((r) => setTimeout(r, 2000 * attempt));
    }
  }
}

let failed = 0;
let offline = false;

for (const file of files) {
  const url = `${base}/${file}`;
  const status = await head(url);
  if (typeof status === "object") {
    console.log(`  skip ${file} — network unreachable (${status.offline})`);
    offline = true;
    continue;
  }
  if (status === 200) {
    console.log(`  ok   ${file}`);
  } else {
    console.error(`  FAIL ${file} — ${status} at ${url}`);
    failed++;
  }
}

if (offline && failed === 0) {
  console.log("check-live-downloads: skipped (no network); nothing proven");
  process.exit(0);
}
if (failed > 0) {
  console.error(
    `\ncheck-live-downloads: ${failed} advertised download(s) do not exist.\n` +
      "The site must not promise files no one can download. Either publish a\n" +
      "release containing them (tag v<version>) before deploying the site, or\n" +
      "remove them from DOWNLOADS.",
  );
  process.exit(1);
}
console.log(`check-live-downloads: ok — all ${files.length} downloads resolve`);
