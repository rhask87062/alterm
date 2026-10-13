// Unit tests for the download page's platform detection.
//
// This logic shipped untested and the bug it hid was subtle: detection "worked"
// but Linux is already first in DOWNLOADS, so the only visible effect for a
// Linux visitor was a faint glow — indistinguishable from no detection at all.
// Real user-agent strings here, not invented ones.
import { detectOs } from "../src/lib/detect-os.js";

const CASES = [
  // [label, navigator.platform, navigator.userAgent, expected]
  [
    "Ubuntu / Firefox",
    "Linux x86_64",
    "Mozilla/5.0 (X11; Ubuntu; Linux x86_64; rv:109.0) Gecko/20100101 Firefox/119.0",
    "linux",
  ],
  [
    "Ubuntu / Chrome",
    "Linux x86_64",
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "linux",
  ],
  [
    "macOS / Safari",
    "MacIntel",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15",
    "macos",
  ],
  [
    "macOS / Chrome",
    "MacIntel",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "macos",
  ],
  [
    "Windows / Chrome",
    "Win32",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    "windows",
  ],
  [
    "Windows / Edge",
    "Win32",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0",
    "windows",
  ],
  // Mobile and unknown platforms have no installer. They must return null so
  // the page falls back to showing everything, rather than recommending a
  // desktop build to a phone.
  [
    "iPhone",
    "iPhone",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
    null,
  ],
  [
    "iPad",
    "iPad",
    "Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
    null,
  ],
  [
    "Android",
    "Linux armv8l",
    "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36",
    null,
  ],
  [
    "ChromeOS",
    "Linux x86_64",
    "Mozilla/5.0 (X11; CrOS x86_64 14541.0.0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    null,
  ],
  ["empty", "", "", null],
  ["garbage", "Amiga", "SomeCrawler/1.0", null],
];

let failed = 0;
for (const [label, platform, ua, expected] of CASES) {
  const got = detectOs(platform, ua);
  if (got === expected) {
    console.log(`  ok   ${label} -> ${got}`);
  } else {
    console.error(`  FAIL ${label} -> ${got}, expected ${expected}`);
    failed++;
  }
}

if (failed > 0) {
  console.error(`check-detect-os: ${failed} of ${CASES.length} failed`);
  process.exit(1);
}
console.log(`check-detect-os: ok — ${CASES.length} cases`);
