# Alterm Website — Design Specification

**Date:** 2026-06-17
**Author:** Russell Alan Haskell + Claude
**Status:** Draft
**Branch:** `worktree-feature+website` (worktree of the `alterm` repo)

## Overview

A marketing website for Alterm with a docs area, built as a static site and
deployed to GitHub Pages. The site sells the vision of Alterm (a
GPU-accelerated, cross-platform terminal workspace in Rust) while honestly
representing its early stage (v0.1.0). It is a **marketing + docs combo**: a
bold landing page backed by a small, growing documentation section.

- **Goal:** Build interest and credibility, give early adopters/contributors a
  clear path (build from source, configure, learn keybindings), and grow into a
  full docs hub as the app matures.
- **Audience:** Developers and terminal enthusiasts (end users + potential
  contributors), spanning beginners, intermediates, and experts.
- **Hosting:** GitHub Pages (static only), tied to the existing
  `github.com/rhask87062/alterm` repo.
- **Visual direction:** "Bold neon-terminal" — dark background, purple→magenta
  gradients, monospace accents, an animated terminal motif.

## Constraints & Success Criteria

- **Static only** — no server code (GitHub Pages constraint).
- **Lightweight & fast** — minimal JS; reinforces Alterm's "no Electron bloat"
  story. Target strong Lighthouse performance.
- **Honest** — clearly distinguish shipped vs. planned features; the hero is a
  stylized demo, not a fake screenshot.
- **Maintainable** — component-based so the docs section can grow without markup
  drift.
- **Accessible** — WCAG AA contrast for body text; respects
  `prefers-reduced-motion`.
- **Robust to missing assets** — the site must look complete before real app
  screenshots exist; screenshots enhance but are not required for launch.

Success = a polished landing page plus three real docs pages, building cleanly
to static output, deployed to GitHub Pages, looking sharp with or without
screenshots.

## Tech Stack

- **Framework:** Astro (static output). Astro components + a single vanilla-JS
  island for the terminal animation. No SPA framework.
- **Styling:** Hand-written CSS with custom properties for design tokens. No CSS
  framework (keeps bundle tiny, on-brand for "lightweight").
- **Fonts:** Self-hosted via `@fontsource` (no third-party requests):
  - Display/headings: **Space Grotesk**
  - Body: **Inter**
  - Mono (code, stats, terminal): **JetBrains Mono**
- **Content:** Astro pages; docs may use MDX / content collections if helpful.
- **Deployment:** GitHub Actions → GitHub Pages.

## Site Structure

```
/                        Landing page (marketing story)
/docs/                   Docs hub (index + sidebar nav)
  /docs/installation     Build-from-source + future packages
  /docs/configuration    config.toml reference (seeded from default.toml)
  /docs/keybindings      Default keymap table (from design spec)
  /docs/ai               AI providers & context model (stub for v1)
  /docs/roadmap          The 5 phases (may live on landing page instead)
```

A shared **Header** (logo + nav + GitHub link) and **Footer** wrap every page as
Astro components. Two layouts: `BaseLayout` (landing/marketing) and `DocsLayout`
(docs sidebar + content).

### v1 Scope

- Full **landing page** (all sections below).
- **Docs hub** with three real, content-seeded pages: **Installation**,
  **Configuration**, **Keybindings**.
- **AI** docs page as a short stub.
- **Roadmap** rendered on the landing page (the 5 phases as a timeline); a
  `/docs/roadmap` page is optional/secondary.

## Landing Page Content (top to bottom)

1. **Header** — Alterm logo (recolored from the black wordmark to read on dark) +
   nav (Features · Docs · Roadmap · GitHub) + "Star on GitHub" button.
2. **Hero** — gradient headline + tagline *"The alternative terminal — for
   beginners, intermediates, and experts."* Subtext: GPU-accelerated,
   cross-platform terminal workspace in Rust — without the Electron bloat. CTAs:
   **Get Started** (→ docs/installation) and **View on GitHub**. Alongside it:
   the **animated terminal mockup** (see below).
3. **Stats band** — proof points in monospace: `< 50MB binary` · `< 1s cold
   start` · `60fps` · `6 AI providers` · `3 platforms`.
4. **Feature grid** — ~9 cards using brand SVG icons where they fit, with honest
   "planned" tags where appropriate:
   GPU rendering · Tiling workspace · Widget sidebar · Keyboard control ·
   AI chat · Embedded browser · File preview · Settings (TOML/Lua) ·
   Cross-platform.
5. **Who it's for** — three columns: Beginners · Intermediates · Experts.
6. **AI section** — the context model (active pane, pinned panes, `@pane`
   mentions) + supported providers (OpenAI, Anthropic, Gemini, xAI/Grok, plus
   local Ollama / LM Studio / any OpenAI-compatible endpoint). A key
   differentiator; gets its own band.
7. **Screenshot gallery** — real app shots once available; graceful placeholder
   until then.
8. **Roadmap** — the 5 phases as a horizontal timeline, current phase (Phase 1)
   marked.
9. **Final CTA** — "Build it from source" + GitHub.
10. **Footer** — GitHub, MIT license, Acknowledgments (the upstream OSS projects
    Alterm builds on), built-with-Rust note.

Order rationale: hook → credibility → substance → fit → differentiator → proof →
vision → action.

## Visual Design System

### Color Tokens (derived from the app icon)

| Token          | Hex       | Use                                        |
|----------------|-----------|--------------------------------------------|
| `bg`           | `#0D0814` | near-black violet page background          |
| `bg-elev`      | `#160E22` | cards, elevated surfaces                   |
| `purple-deep`  | `#280056` | gradient anchor / shadows                  |
| `purple-core`  | `#56008D` | primary brand purple                       |
| `purple-mid`   | `#A021D6` | gradient midpoint                          |
| `orchid`       | `#D450FC` | bright gradient top                        |
| `accent`       | `#F977FF` | magenta accent — CTAs, links, glows, cursor|
| `text`         | `#ECE6F5` | primary text (soft lavender-white)         |
| `text-muted`   | `#9A8FB0` | secondary text                             |

**Signature gradient:** `linear-gradient(135deg, #56008D, #A021D6, #D450FC)` for
headlines, buttons, and terminal-window chrome. Magenta `#F977FF` glow
(box-shadow) on interactive/hover states.

### Typography

- Display/headings: **Space Grotesk**
- Body: **Inter**
- Mono: **JetBrains Mono** (code, stats, terminal demo, prompt accents)

### Motifs

Glowing gradient borders; subtle terminal scanline/grid texture in dark
sections; blinking block cursor (`▋`) accents; monospace labels with `>` prompts
(echoing the ASCII logo's `>>`); rounded corners matching the icon's rounded
square.

### Accessibility

Body text meets WCAG AA on the dark background. Brightest gradient/glow reserved
for large text and decoration. `prefers-reduced-motion` pauses the terminal
animation (shows a completed final frame).

## Animated Terminal Hero (centerpiece)

A hand-built mock of the Alterm window — HTML/CSS plus a small JS typing
animation, wrapped as a single Astro island (the page's only interactive JS).

**Anatomy:**
- **Window chrome** in the brand gradient — rounded corners, three dots
  top-left (matching the icon), subtle magenta glow.
- **A tiling split** — left pane = terminal; right pane = a smaller block
  (mock AI chat or syntax-highlighted file preview). A thin gradient divider
  sells the "drag-to-split" story.
- **Widget sidebar** along one edge using the real SVG icons (terminal, browser,
  folder, settings, info, dark/light).
- **Left pane animation:** looping typed sequence with a blinking `▋` cursor and
  a `>` prompt (e.g. `cargo build --release` → fake colorful build output →
  `alterm`). ANSI-style colors from the palette.
- **Right pane:** static mock (AI exchange or code snippet) — a focal point
  without two competing animations.

**Behavior:** loops gently; pauses on a completed final frame under
`prefers-reduced-motion`; collapses to a clean single-pane view on mobile
(sidebar hides, split stacks). Clearly a stylized demo, not a screenshot.

When real screenshots arrive, they populate the gallery section; the animated
mockup remains the hero regardless, so the top of the page never depends on app
polish.

## Project Structure

The site lives in the repo's `website/` folder:

```
website/
├─ package.json, astro.config.mjs, tsconfig.json
├─ public/                 favicon, icons, og-image, screenshots/
├─ src/
│  ├─ styles/global.css    design tokens, base styles
│  ├─ components/          Header, Footer, FeatureCard, StatBand,
│  │                       RoadmapTimeline, TerminalDemo (island), …
│  ├─ layouts/             BaseLayout.astro, DocsLayout.astro
│  ├─ pages/index.astro    landing page
│  └─ pages/docs/          installation, configuration, keybindings, ai
└─ content/ (optional)     MDX for docs if using content collections
```

## Deployment (GitHub Pages)

- **GitHub Actions workflow** at the repo root: `.github/workflows/deploy-site.yml`
  (must be at root, not inside `website/`). Triggers on website changes; builds
  the Astro site and publishes to Pages.
- **`astro.config.mjs`** sets `site` and `base`. Default Pages URL is
  `https://rhask87062.github.io/alterm/`, so `base: '/alterm/'`. All asset/link
  paths respect `base` so nothing breaks on the sub-path. If a custom domain is
  added later, drop `base`.

## Content Sources (already in the repo)

- **README.md** — features, building, packaging, acknowledgments, roadmap.
- **config/default.toml** — seeds the Configuration docs page.
- **docs/superpowers/specs/2026-04-03-alterm-design.md** — keybindings table,
  AI context model, audience descriptions, stats/success criteria.
- **assets/icons/** — app icon (palette source), wordmark, sidebar SVGs.
- **assets/ascii_logo.txt** — ASCII branding / tagline accent.

## Testing & Verification

- `astro build` succeeds with zero errors.
- `astro check` passes (types/links).
- `astro preview` + a Playwright smoke check: page loads, nav works, no console
  errors, `prefers-reduced-motion` honored, responsive layout holds on mobile
  width.
- Stretch: Lighthouse pass for performance + accessibility.

## Out of Scope (v1)

- Real app screenshots (added to the gallery as they become available).
- Custom domain (default `github.io` sub-path for now).
- Blog/changelog, search, i18n, analytics.
- Full docs coverage beyond Installation/Configuration/Keybindings (+ AI stub).
- Any server-side functionality (newsletter backend, etc.).
```
