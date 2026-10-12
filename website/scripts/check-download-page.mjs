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
  "alterm-linux-amd64.tar.gz",
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

if (failed) process.exit(1);
console.log("check-download-page: ok");
