// Builds nothing — reads the built output. Asserts the download page actually
// renders every advertised asset URL, so a refactor that drops a platform or
// forgets to interpolate RELEASES_BASE is caught instead of shipping.
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const pagePath = resolve(here, "../dist/download/index.html");

let html;
try {
  html = readFileSync(pagePath, "utf8");
} catch {
  console.error(
    `check-download-page: ${pagePath} not found — run 'npm run build' first`,
  );
  process.exit(1);
}

const required = [
  "alterm-linux-amd64.deb",
  "alterm-linux-arm64.deb",
  "alterm-linux-amd64.tar.gz",
  "alterm-linux-arm64.tar.gz",
  "alterm-macos-arm64.pkg",
  "alterm-macos-x86_64.pkg",
  "alterm-windows-x64-setup.exe",
  "SHA256SUMS",
];

let failed = false;
for (const name of required) {
  const href = `https://github.com/rhask87062/alterm/releases/latest/download/${name}`;
  if (html.includes(href)) {
    console.log(`  ok   ${name}`);
  } else {
    console.error(`  FAIL ${name} — no link to ${href}`);
    failed = true;
  }
}

// Every platform must be present without JavaScript: detection only reorders.
for (const os of ["linux", "macos", "windows"]) {
  if (html.includes(`data-os="${os}"`)) {
    console.log(`  ok   ${os} card rendered statically`);
  } else {
    console.error(`  FAIL ${os} card missing from static HTML`);
    failed = true;
  }
}

// Detection must have a *visible* effect. The original bug was that it ran
// correctly but changed almost nothing: Linux is already first in DOWNLOADS,
// so reordering was a no-op and only a faint glow distinguished the match.
// These assert the styling hooks that actually promote and demote exist.
//
// Astro scopes every selector with a [data-astro-cid-xxxx] attribute and
// minifies, so strip the scoping before matching — otherwise the assertions
// would have to encode a build-generated hash.
const css = html.replace(/\[data-astro-cid-[a-z0-9]+\]/g, "");

const stylingHooks = [
  // Promotes the match beyond a border tweak.
  [/\.is-detected h2\s*\{[^}]*font-size/, "detected card gets a larger heading"],
  [
    /\.is-detected \.recommended\s*\{[^}]*display:\s*block/,
    'detected card reveals a "Recommended for you" label',
  ],
  // Demotes the others to a compact row.
  [
    /has-detection \.dl-card:not\(\.is-detected\)\s*\{[^}]*padding/,
    "non-detected cards are compacted",
  ],
  [
    /has-detection \.others-label\s*\{[^}]*display:\s*block/,
    '"Other platforms" label appears only after detection',
  ],
];
for (const [re, label] of stylingHooks) {
  if (re.test(css)) {
    console.log(`  ok   ${label}`);
  } else {
    console.error(`  FAIL ${label} — no matching CSS rule in the built page`);
    failed = true;
  }
}

// The "Recommended" and "Other platforms" affordances must start hidden, so a
// no-JS visitor is never told a platform was detected when none was.
if (/\.recommended,\s*\.others-label\s*\{\s*display:\s*none/.test(css)) {
  console.log("  ok   recommendation affordances hidden without detection");
} else {
  console.error("  FAIL recommendation affordances are not hidden by default");
  failed = true;
}

// The detection module must be wired in (bundled, so look for the import's
// emitted script tag rather than inline source).
if (/<script[^>]*type="module"/.test(html)) {
  console.log("  ok   detection module is loaded");
} else {
  console.error("  FAIL no module script on the page — detection cannot run");
  failed = true;
}

if (failed) process.exit(1);
console.log("check-download-page: ok");
