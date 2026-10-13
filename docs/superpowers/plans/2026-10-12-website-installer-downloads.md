# Website Installer Downloads Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish installers for Linux, macOS and Windows from CI on a version tag, and give the website a `/download` page that links them with URLs that never need editing again.

**Architecture:** A tag-triggered GitHub Actions workflow runs the packaging scripts that already exist in `packaging/`, on each platform's native runner, and publishes the results to a GitHub Release under both versioned names and version-independent aliases. The Astro site is a thin consumer: it links `releases/latest/download/<alias>`, which GitHub resolves by redirect.

**Tech Stack:** GitHub Actions; Bash and PowerShell packaging scripts; Rust/Cargo; Astro 5 + TypeScript for the site; plain Node (no test framework) for the drift check.

**Spec:** `docs/superpowers/specs/2026-10-12-website-installer-downloads-design.md`

## Global Constraints

- Target release version is **0.5.0**, tagged `v0.5.0`. Three files must agree: `Cargo.toml` `[workspace.package]`, `alterm/Cargo.toml`, and `SITE.version` in `website/src/lib/site.ts`.
- The per-crate `version = "0.3.0"` in `crates/terminal`, `crates/gpu-renderer` and `crates/workspace` is internal. **Do not change it** — nothing reads it for packaging.
- Builds ship **unsigned**. No signing steps, no secrets. Every platform that warns on unsigned installers gets a first-launch note in the UI copy.
- The Release must be **published, not a draft, not a prerelease**. `releases/latest/download/` resolves only to a published non-prerelease release.
- These five alias filenames are the contract between CI and the site, and must appear as **literal strings** in `.github/workflows/release.yml` (the drift check greps for them — never build them by string interpolation):
  - `alterm-linux-amd64.deb`
  - `alterm-linux-amd64.tar.gz`
  - `alterm-macos-arm64.pkg`
  - `alterm-macos-x86_64.pkg`
  - `alterm-windows-x64-setup.exe`
- All internal site links go through the existing `url()` helper from `src/lib/site.ts`. The site is served under the `/alterm/` base path, so a bare `/download` 404s. `url()` passes `http`-prefixed strings through unchanged, so absolute GitHub URLs must **not** be wrapped.
- Do not edit `packaging/**`. Those scripts work; this plan only calls them.
- Out of scope, do not add: ARM Linux builds, `.rpm`, Homebrew/Winget/Flatpak, nightly builds, in-app auto-update.

---

### Task 1: Bump the version to 0.5.0

**Files:**
- Modify: `Cargo.toml:15`
- Modify: `alterm/Cargo.toml:3`
- Modify: `website/src/lib/site.ts:10`

**Interfaces:**
- Consumes: nothing.
- Produces: `VERSION=0.5.0` from `packaging/common/metadata.sh`, which every packaging script uses to name its output. Task 2 hardcodes nothing — it reads this value — but Task 2's version guard asserts the tag matches it.

- [ ] **Step 1: Write the failing test**

Create `packaging/tests/version-consistency.sh`:

```bash
#!/bin/bash
# Asserts the three version declarations that must agree all say the same
# thing. They are read by different consumers: metadata.sh (artifact names),
# Cargo's workspace inheritance, and the website's download page.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
fail=0

workspace="$(sed -n 's/^version = "\(.*\)"/\1/p' "${REPO_ROOT}/Cargo.toml" | head -n1)"
binary="$(sed -n 's/^version = "\(.*\)"/\1/p' "${REPO_ROOT}/alterm/Cargo.toml" | head -n1)"
site="$(sed -n 's/.*version: "\(.*\)",.*/\1/p' "${REPO_ROOT}/website/src/lib/site.ts" | head -n1)"

echo "workspace Cargo.toml : ${workspace}"
echo "alterm/Cargo.toml    : ${binary}"
echo "website site.ts      : ${site}"

expected="0.5.0"
for pair in "workspace:${workspace}" "binary:${binary}" "site:${site}"; do
    name="${pair%%:*}"; value="${pair#*:}"
    if [ "${value}" != "${expected}" ]; then
        echo "FAIL: ${name} is '${value}', expected '${expected}'" >&2
        fail=1
    fi
done

# metadata.sh is what actually names the artifacts — check it agrees.
metadata_version="$(cd "${REPO_ROOT}" && source packaging/common/metadata.sh && echo "${VERSION}")"
echo "metadata.sh VERSION  : ${metadata_version}"
if [ "${metadata_version}" != "${expected}" ]; then
    echo "FAIL: metadata.sh reports '${metadata_version}', expected '${expected}'" >&2
    fail=1
fi

[ "${fail}" -eq 0 ] && echo "PASS: all version declarations agree on ${expected}"
exit "${fail}"
```

Make it executable: `chmod +x packaging/tests/version-consistency.sh`

- [ ] **Step 2: Run test to verify it fails**

Run: `./packaging/tests/version-consistency.sh`
Expected: FAIL — all four lines report `0.4.0`, with `FAIL: workspace is '0.4.0', expected '0.5.0'` and three more like it.

- [ ] **Step 3: Write minimal implementation**

In `Cargo.toml`, under `[workspace.package]`:

```toml
version = "0.5.0"
```

In `alterm/Cargo.toml`:

```toml
version = "0.5.0"
```

In `website/src/lib/site.ts`, inside `export const SITE = {`:

```ts
  version: "0.5.0",
```

- [ ] **Step 4: Run test to verify it passes**

Run: `./packaging/tests/version-consistency.sh`
Expected: PASS — `PASS: all version declarations agree on 0.5.0`

- [ ] **Step 5: Verify nothing else broke**

Run: `cargo build --workspace && cargo test --workspace 2>&1 | grep "test result"`
Expected: build succeeds; every line reads `ok.` with `0 failed`. (152 tests pass today.)

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml alterm/Cargo.toml website/src/lib/site.ts packaging/tests/version-consistency.sh
git commit -m "chore(alterm): bump version to 0.5.0 for first published release

Adds a test asserting the three version declarations that must agree —
workspace manifest, binary manifest, and the website's SITE.version — plus
metadata.sh, which derives every artifact name from the binary manifest."
```

---

### Task 2: Release workflow

**Files:**
- Create: `.github/workflows/release.yml`
- Test: `.github/workflows/tests/test-release-workflow.sh`

**Interfaces:**
- Consumes: `VERSION` from `packaging/common/metadata.sh` (Task 1); the existing `packaging/build-release.sh` and `packaging/windows/build-installer.ps1`.
- Produces: a published GitHub Release carrying the five alias filenames listed in Global Constraints, plus versioned names and `SHA256SUMS`. Task 3's drift check greps this file for those literals.

- [ ] **Step 1: Write the failing test**

Create `.github/workflows/tests/test-release-workflow.sh`:

```bash
#!/bin/bash
# Structural checks on release.yml. These catch the mistakes that would
# otherwise only surface as a broken public download: a missing alias, a
# release that publishes before all platforms built, or a draft/prerelease
# flag that makes releases/latest/download/ resolve to nothing.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
WF="${REPO_ROOT}/.github/workflows/release.yml"
fail=0

check() {
    if eval "$2" >/dev/null 2>&1; then
        echo "  ok   $1"
    else
        echo "  FAIL $1" >&2
        fail=1
    fi
}

[ -f "${WF}" ] || { echo "FAIL: ${WF} does not exist" >&2; exit 1; }

echo "release.yml is valid YAML:"
check "parses" "python3 -c \"import yaml,sys; yaml.safe_load(open('${WF}'))\""

echo "triggers:"
check "tag trigger on v*"   "grep -q \"tags:\" '${WF}' && grep -q \"'v\\*'\" '${WF}'"
check "workflow_dispatch"   "grep -q 'workflow_dispatch' '${WF}'"

echo "all five alias filenames appear literally:"
for f in alterm-linux-amd64.deb alterm-linux-amd64.tar.gz \
         alterm-macos-arm64.pkg alterm-macos-x86_64.pkg \
         alterm-windows-x64-setup.exe; do
    check "${f}" "grep -q '${f}' '${WF}'"
done

echo "runners:"
for r in "ubuntu-latest" "macos-latest" "macos-13" "windows-latest"; do
    check "${r}" "grep -q '${r}' '${WF}'"
done

echo "safety properties:"
check "release job needs every build job" \
    "python3 -c \"
import yaml
wf = yaml.safe_load(open('${WF}'))
needs = wf['jobs']['release']['needs']
assert set(['linux','macos-arm','macos-x64','windows']).issubset(set(needs)), needs
\""
check "release is not a draft"      "grep -q 'draft: false' '${WF}'"
check "release is not a prerelease" "grep -q 'prerelease: false' '${WF}'"
check "version guard reads alterm/Cargo.toml" \
    "grep -q 'alterm/Cargo.toml' '${WF}'"
check "publishes only on a tag" \
    "python3 -c \"
import yaml
wf = yaml.safe_load(open('${WF}'))
cond = wf['jobs']['release'].get('if','')
assert 'tag' in cond, cond
\""
check "verifies download URLs after publish" \
    "grep -q 'releases/latest/download' '${WF}'"

[ "${fail}" -eq 0 ] && echo "PASS: release.yml structure is sound"
exit "${fail}"
```

Make it executable: `chmod +x .github/workflows/tests/test-release-workflow.sh`

- [ ] **Step 2: Run test to verify it fails**

Run: `./.github/workflows/tests/test-release-workflow.sh`
Expected: FAIL with `FAIL: .../release.yml does not exist`

- [ ] **Step 3: Write minimal implementation**

Create `.github/workflows/release.yml`:

```yaml
name: Release

# Builds installers for every platform and publishes them to a GitHub Release.
#
# Pushing a v* tag publishes a release. workflow_dispatch is a dry run: it
# builds everything and leaves the artifacts on the run, publishing nothing,
# so the pipeline can be validated before a tag is cut.
#
# Each produced file is uploaded twice — under its versioned name, and under a
# version-independent alias. The website links the aliases via
# releases/latest/download/, so it never needs editing when a version ships.
# Those alias names are a contract checked by website/scripts/check-downloads.mjs;
# keep them as literal strings.
on:
  push:
    tags: ['v*']
  workflow_dispatch:

permissions:
  contents: write

jobs:
  version-guard:
    runs-on: ubuntu-latest
    outputs:
      version: ${{ steps.read.outputs.version }}
    steps:
      - uses: actions/checkout@v4
      - id: read
        name: Read manifest version and compare against the tag
        run: |
          set -euo pipefail
          manifest="$(sed -n 's/^version = "\(.*\)"/\1/p' alterm/Cargo.toml | head -n1)"
          echo "alterm/Cargo.toml version: ${manifest}"
          echo "version=${manifest}" >> "$GITHUB_OUTPUT"
          if [ "${GITHUB_REF_TYPE}" = "tag" ]; then
            tag="${GITHUB_REF_NAME#v}"
            if [ "${tag}" != "${manifest}" ]; then
              echo "::error::Tag v${tag} does not match alterm/Cargo.toml version ${manifest}. Bump the manifest or retag."
              exit 1
            fi
            echo "Tag matches manifest."
          else
            echo "Dry run — skipping tag comparison."
          fi

  linux:
    needs: version-guard
    runs-on: ubuntu-latest
    continue-on-error: ${{ github.ref_type != 'tag' }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Install build dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev build-essential
      - name: Build .deb and portable tarball
        run: ./packaging/build-release.sh linux
      - name: Add stable aliases
        run: |
          set -euo pipefail
          v="${{ needs.version-guard.outputs.version }}"
          cp "dist/alterm_${v}_amd64.deb" "dist/alterm-linux-amd64.deb"
          cp "dist/alterm-${v}-linux-amd64.tar.gz" "dist/alterm-linux-amd64.tar.gz"
          ls -la dist
      - uses: actions/upload-artifact@v4
        with:
          name: dist-linux
          path: dist/*
          if-no-files-found: error

  macos-arm:
    needs: version-guard
    runs-on: macos-latest
    continue-on-error: ${{ github.ref_type != 'tag' }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Build .pkg
        run: ./packaging/build-release.sh macos
      - name: Add stable alias
        run: |
          set -euo pipefail
          v="${{ needs.version-guard.outputs.version }}"
          cp "dist/alterm-${v}-macos-arm64.pkg" "dist/alterm-macos-arm64.pkg"
          ls -la dist
      - uses: actions/upload-artifact@v4
        with:
          name: dist-macos-arm
          path: dist/*.pkg
          if-no-files-found: error

  macos-x64:
    needs: version-guard
    runs-on: macos-13
    continue-on-error: ${{ github.ref_type != 'tag' }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Build .pkg
        run: ./packaging/build-release.sh macos
      - name: Add stable alias
        run: |
          set -euo pipefail
          v="${{ needs.version-guard.outputs.version }}"
          cp "dist/alterm-${v}-macos-x86_64.pkg" "dist/alterm-macos-x86_64.pkg"
          ls -la dist
      - uses: actions/upload-artifact@v4
        with:
          name: dist-macos-x64
          path: dist/*.pkg
          if-no-files-found: error

  windows:
    needs: version-guard
    runs-on: windows-latest
    continue-on-error: ${{ github.ref_type != 'tag' }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Install Inno Setup
        run: choco install innosetup --no-progress -y
      - name: Add Inno Setup to PATH
        shell: pwsh
        run: |
          "C:\Program Files (x86)\Inno Setup 6" |
            Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
      - name: Build setup.exe
        shell: pwsh
        run: pwsh -File .\packaging\windows\build-installer.ps1
      - name: Add stable alias
        shell: pwsh
        run: |
          $v = "${{ needs.version-guard.outputs.version }}"
          Copy-Item "dist\alterm-$v-windows-x64-setup.exe" `
                    "dist\alterm-windows-x64-setup.exe" -Force
          Get-ChildItem dist
      - uses: actions/upload-artifact@v4
        with:
          name: dist-windows
          path: dist/*.exe
          if-no-files-found: error

  release:
    if: github.ref_type == 'tag'
    needs: [version-guard, linux, macos-arm, macos-x64, windows]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/download-artifact@v4
        with:
          path: artifacts
      - name: Collect artifacts and checksum them
        run: |
          set -euo pipefail
          mkdir -p dist
          find artifacts -type f -exec cp -n {} dist/ \;
          cd dist
          # Written outside dist first so the glob can't pick up the file
          # being created.
          sha256sum * > ../SHA256SUMS
          mv ../SHA256SUMS .
          cat SHA256SUMS
      - name: Publish release
        uses: softprops/action-gh-release@v2
        with:
          files: dist/*
          draft: false
          prerelease: false
          generate_release_notes: true
          fail_on_unmatched_files: true
      - name: Verify every advertised download resolves
        run: |
          set -euo pipefail
          # The website links these exact names. If any does not resolve, the
          # download page has a dead button — fail loudly here instead.
          for f in alterm-linux-amd64.deb \
                   alterm-linux-amd64.tar.gz \
                   alterm-macos-arm64.pkg \
                   alterm-macos-x86_64.pkg \
                   alterm-windows-x64-setup.exe; do
            url="https://github.com/${GITHUB_REPOSITORY}/releases/latest/download/${f}"
            code=000
            for attempt in 1 2 3; do
              code="$(curl -sSL -o /dev/null -w '%{http_code}' "${url}" || echo 000)"
              [ "${code}" = "200" ] && break
              echo "attempt ${attempt}: ${code} for ${f}, retrying..."
              sleep 5
            done
            echo "${code}  ${url}"
            if [ "${code}" != "200" ]; then
              echo "::error::${f} is not downloadable at ${url}"
              exit 1
            fi
          done
```

- [ ] **Step 4: Run test to verify it passes**

Run: `./.github/workflows/tests/test-release-workflow.sh`
Expected: PASS — every check prints `ok`, ending `PASS: release.yml structure is sound`

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/release.yml .github/workflows/tests/test-release-workflow.sh
git commit -m "ci(alterm): tag-triggered release workflow for all three platforms

Builds the .deb and portable tarball on ubuntu-latest, both macOS .pkg
variants on arm64 and Intel runners, and setup.exe on windows-latest, then
publishes everything to a GitHub Release.

Every file is uploaded under both its versioned name and a version-independent
alias so the website can link releases/latest/download/ and never need editing
per release. A version guard fails fast when the tag disagrees with
alterm/Cargo.toml, and a post-publish step verifies each advertised URL
actually resolves."
```

---

### Task 3: Download data in `site.ts` plus the drift check

**Files:**
- Modify: `website/src/lib/site.ts` (add `RELEASES_BASE`, `DOWNLOADS`; extend `NAV`; update the Phase 4 `ROADMAP` entry)
- Create: `website/scripts/check-downloads.mjs`
- Modify: `website/package.json` (add the `check:downloads` script)

**Interfaces:**
- Consumes: the five alias filenames in `.github/workflows/release.yml` (Task 2).
- Produces:
  - `RELEASES_BASE: string` — `https://github.com/rhask87062/alterm/releases/latest/download`
  - `DOWNLOADS: DownloadEntry[]` where
    `DownloadEntry = { os: string; label: string; primary: Asset; alt: Asset[] }`
    and `Asset = { file: string; note: string }`.
  - Task 4 imports `RELEASES_BASE` and `DOWNLOADS` from `../lib/site`.

- [ ] **Step 1: Write the failing test**

Create `website/scripts/check-downloads.mjs`. This is both the implementation of the check and the test for the data:

```js
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
```

Add to `website/package.json` scripts:

```json
    "check:downloads": "node scripts/check-downloads.mjs"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd website && npm run check:downloads`
Expected: FAIL with `check-downloads: no DOWNLOADS export found in site.ts`

- [ ] **Step 3: Write minimal implementation**

In `website/src/lib/site.ts`, add after the `SITE` export:

```ts
// Where released installers live. Points at `latest`, so these URLs keep
// working across releases without the site being rebuilt. The filenames are
// version-independent aliases uploaded by .github/workflows/release.yml —
// `npm run check:downloads` asserts the two stay in sync.
export const RELEASES_BASE =
  "https://github.com/rhask87062/alterm/releases/latest/download";

export const DOWNLOADS = [
  {
    os: "linux",
    label: "Linux",
    primary: { file: "alterm-linux-amd64.deb", note: "Debian / Ubuntu" },
    alt: [{ file: "alterm-linux-amd64.tar.gz", note: "portable tarball" }],
  },
  {
    os: "macos",
    label: "macOS",
    // Apple Silicon vs Intel is shown rather than detected: JavaScript cannot
    // tell them apart reliably (Safari misreports it), and handing someone the
    // wrong .pkg is worse than asking them to pick.
    primary: { file: "alterm-macos-arm64.pkg", note: "Apple Silicon" },
    alt: [{ file: "alterm-macos-x86_64.pkg", note: "Intel" }],
  },
  {
    os: "windows",
    label: "Windows",
    primary: { file: "alterm-windows-x64-setup.exe", note: "64-bit" },
    alt: [],
  },
];
```

Extend `NAV` — insert `Download` before `Docs`:

```ts
export const NAV = [
  { label: "Features", href: "/#features" },
  { label: "AI", href: "/#ai" },
  { label: "Roadmap", href: "/#roadmap" },
  { label: "Download", href: "/download" },
  { label: "Docs", href: "/docs" },
];
```

Replace the Phase 4 `ROADMAP` body, which currently claims packaging is in progress:

```ts
  {
    phase: "Phase 4",
    name: "Extras",
    status: "current",
    body: "Embedded browser with in-pane tabs, file preview, and Lua hooks have landed. Prebuilt installers ship for Linux (.deb, tarball), macOS (.pkg) and Windows (.exe); rpm, Homebrew and Winget are still outstanding.",
  },
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd website && npm run check:downloads`
Expected: PASS — `check-downloads: ok — all 5 filenames appear in release.yml`, then the five names listed.

- [ ] **Step 5: Verify the check actually catches drift**

Temporarily break one name, confirm the failure, then restore it:

```bash
cd website
sed -i 's/alterm-macos-arm64.pkg/alterm-macos-aarch64.pkg/' src/lib/site.ts
npm run check:downloads; echo "exit: $?"   # expect failure, exit 1
sed -i 's/alterm-macos-aarch64.pkg/alterm-macos-arm64.pkg/' src/lib/site.ts
npm run check:downloads                     # expect ok again
```

Expected: the middle run prints `- alterm-macos-aarch64.pkg` under "do not appear in" and exits 1. A check that cannot fail is worthless, so this step is not optional.

- [ ] **Step 6: Verify types and build still pass**

Run: `cd website && npm run check && npm run build`
Expected: `astro check` reports 0 errors; the build completes.

- [ ] **Step 7: Commit**

```bash
git add website/src/lib/site.ts website/scripts/check-downloads.mjs website/package.json
git commit -m "feat(website): download data plus a CI check against filename drift

Adds RELEASES_BASE and DOWNLOADS to site.ts, a Download nav entry, and
corrects the Phase 4 roadmap line that still called packaging 'in progress'.

check-downloads.mjs asserts every filename the site links is a name the
release workflow actually uploads. Nothing else catches a mismatch: Astro
builds happily with a wrong string and it surfaces as a 404 for a visitor."
```

---

### Task 4: The `/download` page

**Files:**
- Create: `website/src/pages/download.astro`

**Interfaces:**
- Consumes: `SITE`, `RELEASES_BASE`, `DOWNLOADS`, `url` from `../lib/site` (Task 3); `BaseLayout` from `../layouts/BaseLayout.astro`, which takes `title?: string` and `description?: string`.
- Produces: a page at `/download` (served as `/alterm/download`), linked from `NAV` and from Task 5's hero button.

- [ ] **Step 1: Write the failing test**

Create `website/scripts/check-download-page.mjs`:

```js
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
```

Add to `website/package.json` scripts:

```json
    "check:page": "node scripts/check-download-page.mjs"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd website && npm run build && npm run check:page`
Expected: FAIL with `dist/download/index.html not found — run 'npm run build' first` (the page does not exist yet, so the build produces no such route).

- [ ] **Step 3: Write minimal implementation**

Create `website/src/pages/download.astro`:

```astro
---
import BaseLayout from "../layouts/BaseLayout.astro";
import { SITE, DOWNLOADS, RELEASES_BASE, url } from "../lib/site";

// Builds are unsigned — no Apple or Windows signing certificates. Saying so
// up front is kinder than letting someone hit an OS block and conclude the
// download is broken.
const FIRST_RUN: Record<string, string> = {
  linux:
    "Install the .deb with your package manager, or extract the tarball and run bin/alterm.",
  macos:
    "Unsigned: the first launch needs right-click → Open, then Open again to confirm.",
  windows:
    "Unsigned: SmartScreen may warn. Choose More info → Run anyway.",
};
---

<BaseLayout
  title="Download"
  description={`Download Alterm ${SITE.version} for Linux, macOS, or Windows.`}
>
  <section class="container dl">
    <p class="eyebrow">Download</p>
    <h1>Alterm <span class="ver">v{SITE.version}</span></h1>
    <p class="lede">
      Prebuilt installers for Linux, macOS, and Windows. Builds are unsigned, so
      macOS and Windows show a warning on first launch — the note on each card
      says how to get past it.
    </p>

    <div class="grid" id="dl-grid">
      {
        DOWNLOADS.map((d) => (
          <article class="card dl-card" data-os={d.os}>
            <h2>{d.label}</h2>
            <a class="btn btn-primary" href={`${RELEASES_BASE}/${d.primary.file}`}>
              Download for {d.label}
            </a>
            <p class="fname">
              <code>{d.primary.file}</code>
              <span class="note">{d.primary.note}</span>
            </p>
            {d.alt.length > 0 && (
              <ul class="alts">
                {d.alt.map((a) => (
                  <li>
                    <a href={`${RELEASES_BASE}/${a.file}`}>
                      <code>{a.file}</code>
                    </a>
                    <span class="note">{a.note}</span>
                  </li>
                ))}
              </ul>
            )}
            <p class="firstrun">{FIRST_RUN[d.os]}</p>
          </article>
        ))
      }
    </div>

    <p class="meta">
      <a href={`${RELEASES_BASE}/SHA256SUMS`}>Checksums</a>
      <span aria-hidden="true">·</span>
      <a href={`${SITE.repo}/releases`}>All releases</a>
      <span aria-hidden="true">·</span>
      <a href={url("/docs/installation")}>Build from source</a>
    </p>
  </section>

  <script is:inline>
    // Progressive enhancement only: moves the visitor's platform to the front
    // and marks it primary. Every platform is already in the HTML, so with no
    // JS (or an unrecognised platform) this page is a plain three-card list.
    (function () {
      var probe = (navigator.platform || "") + " " + (navigator.userAgent || "");
      var os = /Win/i.test(probe)
        ? "windows"
        : /Mac|iPhone|iPad/i.test(probe)
          ? "macos"
          : /Linux|X11|Android|CrOS/i.test(probe)
            ? "linux"
            : null;
      if (!os) return;
      var grid = document.getElementById("dl-grid");
      if (!grid) return;
      var card = grid.querySelector('[data-os="' + os + '"]');
      if (!card) return;
      grid.prepend(card);
      card.classList.add("is-detected");
    })();
  </script>
</BaseLayout>

<style>
  .dl {
    padding: clamp(40px, 6vw, 80px) 0 clamp(56px, 8vw, 96px);
  }
  .dl h1 {
    font-family: var(--font-display);
    font-size: clamp(2rem, 4.4vw, 3rem);
    margin: 0 0 10px;
  }
  .ver {
    font-family: var(--font-mono);
    font-size: 0.5em;
    color: var(--text-muted);
    vertical-align: middle;
  }
  .lede {
    color: var(--text-muted);
    max-width: 62ch;
    margin: 0 0 34px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 18px;
  }
  .dl-card {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 22px;
  }
  .dl-card h2 {
    font-family: var(--font-display);
    font-size: 1.15rem;
    margin: 0;
  }
  /* The detected platform reads as the recommended one. Without JS no card
     carries this class and all three stay visually equal. */
  .dl-card.is-detected {
    box-shadow: var(--glow);
    border-color: var(--line-bright);
  }
  .fname {
    margin: 0;
    font-size: 0.82rem;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: baseline;
  }
  .fname code,
  .alts code {
    font-family: var(--font-mono);
    font-size: 0.95em;
    color: var(--text-muted);
    overflow-wrap: anywhere;
  }
  .note {
    color: var(--text-faint);
    font-size: 0.8rem;
  }
  .alts {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 0.82rem;
  }
  .alts li {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: baseline;
  }
  .firstrun {
    margin: auto 0 0;
    padding-top: 10px;
    border-top: 1px solid var(--line);
    color: var(--text-faint);
    font-size: 0.8rem;
  }
  .meta {
    margin: 30px 0 0;
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    font-size: 0.86rem;
    color: var(--text-muted);
  }
</style>
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd website && npm run build && npm run check:page`
Expected: PASS — nine `ok` lines (six asset links, three platform cards) ending `check-download-page: ok`

- [ ] **Step 5: Verify types and the drift check**

Run: `cd website && npm run check && npm run check:downloads`
Expected: `astro check` reports 0 errors; the drift check still reports ok for all 5.

- [ ] **Step 6: Commit**

```bash
git add website/src/pages/download.astro website/scripts/check-download-page.mjs website/package.json
git commit -m "feat(website): /download page with per-platform installers

Renders all three platforms statically from DOWNLOADS; a small inline script
only promotes the visitor's platform, so with JS off the page is a complete
three-card list rather than an empty one.

macOS shows Apple Silicon and Intel side by side instead of guessing — JS
cannot distinguish them reliably and the wrong .pkg is worse than a choice.
Unsigned-build first-launch notes are stated on each card.

check-download-page.mjs asserts the built HTML really contains every asset URL
and all three platform cards."
```

---

### Task 5: Point the site at the new page

**Files:**
- Modify: `website/src/pages/index.astro:36-42` (hero primary button)
- Modify: `website/src/pages/docs/installation.astro:9-12` and `:55-61`

**Interfaces:**
- Consumes: the `/download` route from Task 4.
- Produces: no new exports.

- [ ] **Step 1: Write the failing test**

Create `website/scripts/check-install-copy.mjs`:

```js
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
for (const [label, src] of [["home", home], ["installation", install]]) {
  if (/currently distributed as source/i.test(src))
    bad(`${label} still says "currently distributed as source"`);
  else ok(`${label} has no source-only claim`);

  if (/Planned distribution/i.test(src))
    bad(`${label} still has a "Planned distribution" section`);
  else ok(`${label} has no "Planned distribution" section`);
}

// Both entry points must link the download page.
for (const [label, src] of [["home", home], ["installation", install]]) {
  if (/href="[^"]*\/download"/.test(src)) ok(`${label} links /download`);
  else bad(`${label} does not link /download`);
}

if (failed) process.exit(1);
console.log("check-install-copy: ok");
```

Add to `website/package.json` scripts:

```json
    "check:copy": "node scripts/check-install-copy.mjs"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd website && npm run build && npm run check:copy`
Expected: FAIL — `installation still says "currently distributed as source"`, `installation still has a "Planned distribution" section`, and `home does not link /download`.

- [ ] **Step 3: Write minimal implementation**

In `website/src/pages/index.astro`, change the hero primary button from `Get started` → installation, to point at the download page. Replace:

```astro
          <a class="btn btn-primary" href={url("/docs/installation")}>
            Get started
```

with:

```astro
          <a class="btn btn-primary" href={url("/download")}>
            Download
```

Leave the SVG, the GitHub ghost button, and the closing-section "Installation guide" button exactly as they are.

In `website/src/pages/docs/installation.astro`, replace the opening paragraph (lines 9–12):

```astro
  <p>
    Alterm is currently distributed as source. Building it takes one Cargo command. Packaged
    installers (<code>.deb</code>, <code>.rpm</code>, Homebrew, Winget) are on the roadmap.
  </p>
```

with:

```astro
  <p>
    Prebuilt installers are available for Linux, macOS, and Windows on the
    <a href={url("/download")}>download page</a>. Builds are unsigned, so macOS and
    Windows warn on first launch — that page says how to get past it.
  </p>
  <p>
    Prefer to build it yourself? That takes one Cargo command.
  </p>
```

Add the `url` import at the top of the same file's frontmatter, since it is not imported today:

```astro
---
import DocsLayout from "../../layouts/DocsLayout.astro";
import { url } from "../../lib/site";
---
```

Replace the "Planned distribution" section (lines 55–61) with an accurate one:

```astro
  <h2>Not yet packaged</h2>
  <ul>
    <li><code>.rpm</code> (Fedora / RHEL)</li>
    <li>Homebrew formula (macOS)</li>
    <li>Winget / MSIX (Windows)</li>
    <li>Flatpak (cross-distro Linux)</li>
  </ul>
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd website && npm run build && npm run check:copy`
Expected: PASS — six `ok` lines (two stale-claim checks per page, plus one
`/download` link check per page) ending `check-install-copy: ok`

- [ ] **Step 5: Verify the whole site still checks out**

Run: `cd website && npm run check && npm run check:downloads && npm run check:page && npm run check:copy`
Expected: all four pass, 0 type errors.

- [ ] **Step 6: Commit**

```bash
git add website/src/pages/index.astro website/src/pages/docs/installation.astro website/scripts/check-install-copy.mjs website/package.json
git commit -m "docs(website): lead with installers instead of build-from-source

The hero now points at /download, and the installation page no longer claims
Alterm is 'currently distributed as source' or lists .deb as planned — both
untrue once installers ship. Build-from-source stays, as the second option.

check-install-copy.mjs asserts the stale claims are gone and both entry points
link the download page, so this copy cannot quietly rot again."
```

---

### Task 6: Run the site checks in CI

**Files:**
- Modify: `.github/workflows/deploy-site.yml` (add a check step before build)

**Interfaces:**
- Consumes: the `check:downloads`, `check:page` and `check:copy` scripts (Tasks 3–5).
- Produces: nothing importable.

- [ ] **Step 1: Write the failing test**

In `.github/workflows/tests/test-release-workflow.sh`, replace the final two lines:

```bash
[ "${fail}" -eq 0 ] && echo "PASS: release.yml structure is sound"
exit "${fail}"
```

with the deploy-site checks plus a message that covers both files:

```bash
DEPLOY="${REPO_ROOT}/.github/workflows/deploy-site.yml"
echo "deploy-site.yml runs the download checks:"
check "parses"           "python3 -c \"import yaml; yaml.safe_load(open('${DEPLOY}'))\""
check "check:downloads"  "grep -q 'check:downloads' '${DEPLOY}'"
check "check:page"       "grep -q 'check:page' '${DEPLOY}'"
check "check:copy"       "grep -q 'check:copy' '${DEPLOY}'"

[ "${fail}" -eq 0 ] && echo "PASS: release and deploy workflow structure is sound"
exit "${fail}"
```

- [ ] **Step 2: Run test to verify it fails**

Run: `./.github/workflows/tests/test-release-workflow.sh`
Expected: FAIL — `FAIL check:downloads`, `FAIL check:page`, `FAIL check:copy` (release.yml checks still pass).

- [ ] **Step 3: Write minimal implementation**

In `.github/workflows/deploy-site.yml`, inside the `build` job, replace:

```yaml
      - run: npm ci
      - run: npm run build
```

with:

```yaml
      - run: npm ci
      # Fails the deploy if the site links an installer the release workflow
      # does not upload — a dead download button is worse than a late deploy.
      - run: npm run check:downloads
      - run: npm run build
      - run: npm run check:page
      - run: npm run check:copy
```

`check:page` and `check:copy` read `dist/`, so they must run after the build; `check:downloads` reads source and runs before it.

Also add `.github/workflows/release.yml` to the workflow's `paths` trigger, so a change to the alias names re-runs the drift check:

```yaml
    paths:
      - "website/**"
      - ".github/workflows/deploy-site.yml"
      - ".github/workflows/release.yml"
```

- [ ] **Step 4: Run test to verify it passes**

Run: `./.github/workflows/tests/test-release-workflow.sh`
Expected: PASS — every check `ok`, including the four new `deploy-site.yml` ones.

- [ ] **Step 5: Verify every check passes locally, as CI will run them**

```bash
cd website && npm run check:downloads && npm run build && npm run check:page && npm run check:copy
cd .. && ./packaging/tests/version-consistency.sh && ./.github/workflows/tests/test-release-workflow.sh
```

Expected: all six pass.

- [ ] **Step 6: Commit**

```bash
git add .github/workflows/deploy-site.yml .github/workflows/tests/test-release-workflow.sh
git commit -m "ci(website): gate the site deploy on the download checks

Runs check:downloads before the build and check:page / check:copy after it, so
a deploy cannot publish a page linking installers the release never uploads.
Adds release.yml to the trigger paths so renaming an alias re-runs the check."
```

---

## Post-implementation: cutting the actual release

These steps are deliberately **not** tasks — they publish to the public and need a human decision. Do them in this order; the spec's rollout section explains why.

1. **Push the branch and merge to `main`.** The workflow must exist on the default branch before a tag can trigger it.
2. **Dry run.** `gh workflow run release.yml --ref main`, then watch `gh run watch`. Fix anything that breaks — the dry run has `continue-on-error` on build jobs, so one run surfaces every platform's failure.
3. **Download and inspect the dry-run artifacts.** `gh run download <id>`. Install the `.deb` locally (`sudo dpkg -i`) and launch it.
   **The `.pkg` and `setup.exe` cannot be verified from Linux.** CI producing them is not evidence they install. They stay unverified until someone runs them on macOS and Windows.
4. **Tag and push.** `git tag v0.5.0 && git push origin v0.5.0`. The version guard will reject the tag if the manifest disagrees.
5. **Confirm the Release.** Check it is published, not a draft or prerelease, and that the post-publish URL verification step passed.
6. **Confirm the site.** The merge in step 1 already triggered `deploy-site.yml`; visit `/alterm/download` and click one link per platform.

If step 6 happens before step 4, every button on the page 404s — hence the ordering.

## Known follow-on work

- **Launch-time version-check notice** — agreed as the next spec: on startup, compare `SITE.version` against the GitHub releases API and surface "v0.6.0 available" with a link to the download page. No downloading, no binary replacement.
- Signing/notarization, `.rpm`, Homebrew, Winget, Flatpak, ARM Linux — all explicitly out of scope here, all additive later.
