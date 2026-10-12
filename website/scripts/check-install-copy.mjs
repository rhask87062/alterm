// The installation docs claimed Alterm was source-only and listed .deb as
// "planned". Both became false the moment installers shipped. This asserts
// the stale claims are gone and the download page is linked from the places a
// visitor starts.
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const dist = resolve(here, "../dist");

function read(p, label) {
  try {
    return readFileSync(resolve(dist, p), "utf8");
  } catch {
    console.error(`check-install-copy: ${label} not built at ${p}`);
    process.exit(1);
  }
}

const home = read("index.html", "home page");
const install = read("docs/installation/index.html", "installation page");

let failed = false;
const ok = (m) => console.log(`  ok   ${m}`);
const bad = (m) => {
  console.error(`  FAIL ${m}`);
  failed = true;
};

// Stale claims must be gone.
for (const [label, src] of [
  ["home", home],
  ["installation", install],
]) {
  if (/currently distributed as source/i.test(src))
    bad(`${label} still says "currently distributed as source"`);
  else ok(`${label} has no source-only claim`);

  if (/Planned distribution/i.test(src))
    bad(`${label} still has a "Planned distribution" section`);
  else ok(`${label} has no "Planned distribution" section`);
}

// Both entry points must point at the download page from their *content*.
// A bare href check is not enough: the Download nav entry renders in the
// header of every page, so it would pass without either change being made.

// The home page's hero call-to-action must be the download button.
if (/<a[^>]*class="btn btn-primary"[^>]*href="[^"]*\/download"/.test(home))
  ok("home hero primary button points at /download");
else bad("home hero primary button does not point at /download");

// The installation page must link the download page in prose, not just nav.
if (/<a[^>]*href="[^"]*\/download"[^>]*>download page<\/a>/i.test(install))
  ok('installation body links the "download page"');
else bad('installation body has no "download page" link');

if (failed) process.exit(1);
console.log("check-install-copy: ok");
