# Website Installer Downloads — Design

**Date:** 2026-10-12
**Status:** Designed (not yet implemented)
**Branch:** `feature/website-installer-downloads`
**Problem:** Alterm has working packaging scripts for all three platforms, but
no installer has ever been published. `git tag` and `gh release list` are both
empty, so there is nothing anywhere for a visitor to download. The website
reflects this — `docs/installation.astro` says Alterm "is currently distributed
as source" and lists `.deb` / Homebrew / Winget under "Planned distribution."
A visitor who wants to try Alterm must install Rust and build it.

## Goals

- A visitor can download a working installer for their platform in one click.
- Artifacts are produced reproducibly in CI, not by hand on three machines.
- The site's download links never need editing when a new version ships.
- Releases stay deliberate: a release happens when the maintainer decides, not
  on every push.

## Non-goals (deferred)

- **In-app auto-update.** Installed copies do not update themselves and this
  design does not change that. A smaller follow-on piece — a launch-time
  version-check notice ("v0.6.0 available", linking to the download page, with
  no downloading or binary replacement) — is the agreed next spec.
- ~~**ARM Linux builds.**~~ **Implemented in 0.5.1.** This was deferred on the
  reasoning that `ubuntu-latest` is x86_64 only, which was out of date: GitHub
  made arm64 Linux runners generally available and free for public
  repositories in August 2025. Deferring it also turned out to be the bug that
  mattered — the first user to try the `.deb` was on arm64 Ubuntu, and apt
  refused the amd64 package. Linux now builds natively on both
  `ubuntu-24.04` and `ubuntu-24.04-arm`.
- **`.rpm` packages.** No build script exists; writing one is separate work.
- **Code signing / notarization.** No Apple or Windows certificates are
  available, so builds ship unsigned (see Error handling). Adding signing later
  requires only CI secrets plus signing steps — no redesign.
- **Homebrew / Winget / Flatpak.** Out of scope.
- **Nightly builds from `main`.** Considered and rejected; see Approach.

## Approach (chosen: CI builds all platforms on tag, GitHub Releases hosts)

Alternatives considered and rejected:

- **Manual build and upload** — needs a Mac and a Windows machine for every
  release, with manual steps that are easy to get wrong or skip. macOS and
  Windows artifacts would reliably lag behind Linux.
- **Link only to the GitHub Releases page** — smallest change, but pushes
  platform and file-format choice onto the visitor, which is exactly the
  friction this is meant to remove.
- **A release on every push to `main`** — the maintainer's initial assumption.
  Rejected: artifact names come from the version in `alterm/Cargo.toml`, so
  every release would be named identically unless the version were bumped each
  commit, and users would get constant churn from work-in-progress commits.
- **Nightly pre-release alongside tagged releases** — roughly doubles CI
  minutes and adds a rolling pre-release to maintain, for no current need.

The release pipeline is the source of truth for artifacts. The website is a
thin consumer that links to stable, version-independent URLs.

## Design

### 1. Release workflow (`.github/workflows/release.yml`)

**Triggers:**

- `push` on tags matching `v*` — builds and publishes a GitHub Release.
- `workflow_dispatch` — dry run. Builds and uploads run artifacts but publishes
  no Release, for validating the pipeline before cutting a real tag.

**Version guard (first step).** Compare the tag (`v0.5.0` → `0.5.0`) against
the version in `alterm/Cargo.toml` (the same file `packaging/common/metadata.sh`
reads) and fail fast on mismatch. Without this, a tag pushed before the manifest
bump silently produces artifacts named with the old version.

**Build matrix.** Each job runs the packaging script that already exists, so
this workflow adds CI plumbing, not packaging logic:

| Job | Runner | Setup | Produces |
|---|---|---|---|
| `linux` | `ubuntu-latest` | `apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev build-essential` | `alterm_<v>_amd64.deb`, `alterm-<v>-linux-amd64.tar.gz` |
| `macos-arm` | `macos-latest` (arm64) | none (`pkgbuild` is built in) | `alterm-<v>-macos-arm64.pkg` |
| `macos-x64` | `macos-13` (Intel) | none | `alterm-<v>-macos-x86_64.pkg` |
| `windows` | `windows-latest` | `choco install innosetup` | `alterm-<v>-windows-x64-setup.exe` |

Linux runs `./packaging/build-release.sh linux`, macOS runs
`./packaging/build-release.sh macos`, Windows runs
`pwsh -File .\packaging\windows\build-installer.ps1`.

**Stable alias names.** Artifact names embed the version, so
`/releases/latest/download/alterm_0.5.0_amd64.deb` breaks the moment 0.6.0
ships. Every produced file is therefore uploaded **twice** — once under its
versioned name (for anyone pinning a specific build) and once under a
version-independent alias. Five files across the four jobs, so ten assets plus
`SHA256SUMS`:

```
alterm-linux-amd64.deb        alterm-macos-arm64.pkg
alterm-linux-amd64.tar.gz     alterm-macos-x86_64.pkg
                              alterm-windows-x64-setup.exe
```

The site links `https://github.com/rhask87062/alterm/releases/latest/download/<alias>`,
which GitHub resolves by redirect. No rebuild, no API call, no per-release edit.

The aliases must appear as literal strings in `release.yml` — the drift check
in Testing greps for them, so building a name by string interpolation would
defeat it.

**Release job.** `needs` all four build jobs. Downloads every artifact,
generates `SHA256SUMS`, and publishes the Release on the tag with all versioned
assets, all aliases, and the checksums file attached.

The Release must be **published, not a draft, and not flagged as a
prerelease** — `/releases/latest/download/` resolves only to a published,
non-prerelease release. A draft or prerelease would leave every link on the
site broken.

### 2. Site data (`website/src/lib/site.ts`)

`site.ts` states in its own header that content lives there and components stay
presentation-only, so the artifact list goes there:

```ts
export const RELEASES_BASE =
  "https://github.com/rhask87062/alterm/releases/latest/download";

export const DOWNLOADS = [
  { os: "linux", label: "Linux",
    primary: { file: "alterm-linux-amd64.deb", note: "Debian / Ubuntu" },
    alt: [{ file: "alterm-linux-amd64.tar.gz", note: "portable" }] },
  { os: "macos", label: "macOS",
    primary: { file: "alterm-macos-arm64.pkg", note: "Apple Silicon" },
    alt: [{ file: "alterm-macos-x86_64.pkg", note: "Intel" }] },
  { os: "windows", label: "Windows",
    primary: { file: "alterm-windows-x64-setup.exe", note: "64-bit" },
    alt: [] },
];
```

Also in `site.ts`:

- Add `{ label: "Download", href: "/download" }` to `NAV`.
- Update the Phase 4 `ROADMAP` entry, which currently claims packaging is "in
  progress". Once this ships, `.deb` / `.pkg` / `.exe` are done; `.rpm`,
  Homebrew and Winget remain outstanding.

### 3. Download page (`website/src/pages/download.astro`)

Renders all three platforms statically from `DOWNLOADS`. Shows `SITE.version`
as the heading version, a checksums link, a link to build-from-source, and a
link to the full Releases page.

**OS detection is progressive enhancement, never a gate.** The static HTML
lists every platform; a small inline script then promotes the match by moving
it first and styling it as primary. With JS disabled, or if detection fails,
the visitor sees the complete list — identical to the no-JS baseline. Nothing
is hidden by default.

**CPU architecture is always shown, never detected.** A browser cannot
reliably report the CPU it is running on: Safari misreports Apple Silicon as
Intel, and Linux user agents routinely omit the architecture. Every platform
therefore lists its architectures as sibling buttons (`variants` in
`DOWNLOADS`) — x86-64 and ARM64 for Linux, Apple Silicon and Intel for macOS.
Only the *operating system* is detected. Guessing an architecture hands
someone a package that cannot install, which is precisely what happened in
0.5.0 when an arm64 Ubuntu user was offered the amd64 `.deb`.

All internal links go through the existing `url()` helper — the site is served
under the `/alterm/` base path, so a bare `/download` would 404. The GitHub
asset URLs are absolute, and `url()` passes `http`-prefixed strings through
unchanged.

### 4. Existing page edits

- **`website/src/pages/index.astro`** — hero primary button becomes
  "Download" → `/download`. The GitHub ghost button stays. The closing
  section's "Installation guide" button keeps pointing at the docs.
- **`website/src/pages/docs/installation.astro`** — remove the "currently
  distributed as source" opening and the "Planned distribution" list, both now
  false. Lead with a link to `/download`; keep build-from-source; keep the
  `packaging/` section, which is still useful to contributors.

### 5. Error handling / edge cases

- **Unsigned builds.** macOS Gatekeeper blocks unsigned installers behind
  "unidentified developer" and Windows SmartScreen warns. Both the download
  page and the installation docs carry a short per-OS first-launch note:
  right-click → Open on macOS, "More info → Run anyway" on Windows, nothing
  needed on Linux. This is stated plainly rather than hidden — a visitor who
  hits an unexplained block assumes the download is broken.
- **Partial build failure.** The `release` job `needs` all four builds, so a
  broken Windows build publishes no Release at all. A delayed release is better
  than a live page with one dead button. The `workflow_dispatch` dry run
  inverts this with `continue-on-error` on the build jobs, surfacing every
  platform's failure in one run instead of one per run.
- **Filename drift.** The alias names exist in both `release.yml` and
  `DOWNLOADS`. If they disagree the site shows a 404 button, and neither build
  catches it. Guarded two ways; see Testing.
- **Missing or failed upload.** Covered by the post-publish URL check; see
  Testing.

### 6. Testing

- **`npm run check:downloads`** — a plain Node script (no new test framework)
  asserting every filename in `DOWNLOADS` appears in `release.yml`. Runs in the
  site's CI job. This is the main defence against filename drift.
- **Post-publish URL check** — once the Release is published, the release job
  `curl -IL`s each `/releases/latest/download/<alias>` URL and fails if any
  does not return 200. The job already knows the names, so this is a few lines,
  and it catches both a broken upload and an accidental draft/prerelease flag
  before a user does.
- **`npm run build`** — the Astro build must pass, as it does today via
  `deploy-site.yml`.
- **Dry run before the first real tag** — run `workflow_dispatch`, download all
  five files from the run's artifacts, and install at least the `.deb` locally.
- **Not tested:** no browser-automation test for the OS-detection script. It is
  ~15 lines whose failure mode is "you see all three platforms", which is the
  no-JS baseline. A test harness is not worth it.

**Verification limit, stated explicitly:** the `.pkg` and `setup.exe` cannot be
installed or smoke-tested from the Linux development machine. CI producing them
is not evidence that they install correctly. They stay unverified until someone
runs them on macOS and Windows.

### 7. Rollout order

The download page's links resolve only once a Release exists, so order matters:

1. Bump the version to `0.5.0` in `Cargo.toml` (`[workspace.package]`),
   `alterm/Cargo.toml`, and `SITE.version` in `website/src/lib/site.ts`. The
   three must agree: the first feeds nothing user-facing, the second drives
   every artifact name via `packaging/common/metadata.sh`, and the third is
   what the download page displays. The per-crate `0.3.0` versions on
   `terminal`, `gpu-renderer` and `workspace` are internal and stay as they
   are — nothing reads them for packaging.
2. Add `release.yml`; run the dry run; fix what breaks.
3. Tag `v0.5.0` and push it — first real Release publishes.
4. Merge the website changes, which triggers the existing `deploy-site.yml`.

Doing step 4 before step 3 publishes a page of 404s.

`v0.5.0` rather than `v0.4.0`: 0.4.0 was never cut as a release, and this is
the first published build, so it ships the browser-tabs work under a fresh
minor version rather than retroactively claiming an unreleased number.

## Risks / contingencies

- **`libwebkit2gtk-4.1-dev` unavailable on `ubuntu-latest`.** Package naming
  has shifted across Ubuntu releases. If the current runner image lacks 4.1,
  pin the runner (e.g. `ubuntu-24.04`) rather than loosening the version, since
  `crates/browser` pins `webkit2gtk = "=2.0.2"` with feature `v2_38`.
- **Intel macOS runner retirement.** This already bit us: the first dry run
  used `macos-13`, which GitHub retired in December 2025. A job targeting a
  retired label is *accepted and queued indefinitely* rather than failed, so it
  presents as a slow build — the first dry run sat queued for an hour with no
  runner assigned while the other three platforms finished. The Intel runner is
  now `macos-15-intel`, and the structural test asserts no retired label is
  used. GitHub drops x86_64 macOS entirely when the macOS 15 image retires in
  Fall 2027; at that point either cross-compile from arm64 or drop the Intel
  `.pkg`, which the alias name makes a site-side one-liner.
- **Inno Setup via Chocolatey.** If `choco install innosetup` proves flaky,
  switch to the official installer download or a pinned Chocolatey version.
- **Unsigned-download bounce rate.** Some visitors will refuse an unsigned
  installer regardless of instructions. The mitigation is signing, which this
  design is structured to accept later without rework.
