// Guards the contract between the release workflow and the website: every
// filename the download page links must be a file the release actually
// uploads. Nothing else catches a mismatch — Astro builds fine with a wrong
// string, and the breakage only shows up as a 404 for a visitor.
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "../..");
const sitePath = resolve(repoRoot, "website/src/lib/site.ts");
const workflowPath = resolve(repoRoot, ".github/workflows/release.yml");

function die(msg) {
  console.error(`check-downloads: ${msg}`);
  process.exit(1);
}

let siteSrc, workflowSrc;
try {
  siteSrc = readFileSync(sitePath, "utf8");
} catch {
  die(`cannot read ${sitePath}`);
}
try {
  workflowSrc = readFileSync(workflowPath, "utf8");
} catch {
  die(`cannot read ${workflowPath} — the release workflow must exist`);
}

const start = siteSrc.indexOf("export const DOWNLOADS");
if (start === -1) die("no DOWNLOADS export found in site.ts");
const end = siteSrc.indexOf("\n];", start);
if (end === -1) die("DOWNLOADS export is not terminated by '];'");

const block = siteSrc.slice(start, end);
const files = [...block.matchAll(/file:\s*"([^"]+)"/g)].map((m) => m[1]);
if (files.length === 0) die("DOWNLOADS contains no file entries");

const missing = files.filter((f) => !workflowSrc.includes(f));
if (missing.length > 0) {
  console.error(
    "check-downloads: these DOWNLOADS filenames do not appear in .github/workflows/release.yml:",
  );
  for (const f of missing) console.error(`  - ${f}`);
  console.error(
    "The site would link assets the release never uploads. Fix the name in\n" +
      "site.ts, or add the alias to release.yml.",
  );
  process.exit(1);
}

console.log(
  `check-downloads: ok — all ${files.length} filenames appear in release.yml`,
);
for (const f of files) console.log(`  - ${f}`);
