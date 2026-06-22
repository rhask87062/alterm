/* ============================================================================
   Alterm landing-page motion controller
   ----------------------------------------------------------------------------
   Every effect degrades gracefully:
     • prefers-reduced-motion → reveals/counters jump to final, pointer FX off
     • coarse pointer (touch)  → pointer FX off (tilt, spotlight, cursor glow)
     • no JS                   → server-rendered content stays fully visible
   All hot paths are transform/opacity only and rAF-throttled — no layout thrash.
   ========================================================================== */

const mqReduce = window.matchMedia("(prefers-reduced-motion: reduce)");
const mqFine = window.matchMedia("(hover: hover) and (pointer: fine)");

const reduced = () => mqReduce.matches;
const finePointer = () => mqFine.matches;

/* -------------------------------------------------------------------------- */
/* Scroll reveal — rise + blur-in, directional variants, index-staggered      */
/* -------------------------------------------------------------------------- */
function initReveals(): void {
  const els = Array.from(document.querySelectorAll<HTMLElement>(".reveal"));
  if (!els.length) return;

  if (reduced() || !("IntersectionObserver" in window)) {
    els.forEach((el) => el.classList.add("is-in"));
    return;
  }

  const io = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        const el = e.target as HTMLElement;
        el.style.transitionDelay = `${el.dataset.delay ?? "0"}ms`;
        el.classList.add("is-in");
        io.unobserve(el);
      }
    },
    { rootMargin: "0px 0px -8% 0px", threshold: 0.12 }
  );
  els.forEach((el) => io.observe(el));
}

/* -------------------------------------------------------------------------- */
/* Scroll-progress beam across the very top of the page                       */
/* -------------------------------------------------------------------------- */
function initScrollProgress(): void {
  const bar = document.querySelector<HTMLElement>(".scroll-progress");
  if (!bar) return;

  let ticking = false;
  const update = (): void => {
    const doc = document.documentElement;
    const max = doc.scrollHeight - doc.clientHeight;
    const p = max > 0 ? doc.scrollTop / max : 0;
    bar.style.transform = `scaleX(${p})`;
    ticking = false;
  };
  const onScroll = (): void => {
    if (ticking) return;
    ticking = true;
    requestAnimationFrame(update);
  };

  window.addEventListener("scroll", onScroll, { passive: true });
  window.addEventListener("resize", onScroll, { passive: true });
  update();
}

/* -------------------------------------------------------------------------- */
/* Count-up stats — animate a leading number, preserve prefix/suffix          */
/* -------------------------------------------------------------------------- */
interface ParsedNumber {
  pre: string;
  post: string;
  target: number;
  decimals: number;
}

function parseNumber(raw: string): ParsedNumber | null {
  const m = raw.match(/-?\d[\d,]*\.?\d*/);
  if (!m || m.index === undefined) return null;
  const token = m[0];
  return {
    pre: raw.slice(0, m.index),
    post: raw.slice(m.index + token.length),
    target: parseFloat(token.replace(/,/g, "")),
    decimals: (token.split(".")[1] ?? "").length,
  };
}

function initCounters(): void {
  const els = Array.from(document.querySelectorAll<HTMLElement>("[data-counter]"));
  if (!els.length) return;

  const easeOutCubic = (t: number): number => 1 - Math.pow(1 - t, 3);

  const run = (el: HTMLElement): void => {
    const raw = el.dataset.counter ?? el.textContent ?? "";
    const info = parseNumber(raw);
    if (!info || reduced()) {
      el.textContent = raw;
      return;
    }
    const duration = 1400;
    let start: number | null = null;
    const tick = (ts: number): void => {
      if (start === null) start = ts;
      const t = Math.min(1, (ts - start) / duration);
      const value = info.target * easeOutCubic(t);
      el.textContent = info.pre + value.toFixed(info.decimals) + info.post;
      if (t < 1) requestAnimationFrame(tick);
      else el.textContent = raw;
    };
    requestAnimationFrame(tick);
  };

  // Pre-seed the zero state (JS, motion allowed) so there's no flash of the
  // final value before the count begins.
  if (!reduced()) {
    for (const el of els) {
      const info = parseNumber(el.dataset.counter ?? el.textContent ?? "");
      if (info) el.textContent = info.pre + (0).toFixed(info.decimals) + info.post;
    }
  }

  if (!("IntersectionObserver" in window)) {
    els.forEach(run);
    return;
  }
  const io = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        run(e.target as HTMLElement);
        io.unobserve(e.target);
      }
    },
    { threshold: 0.6 }
  );
  els.forEach((el) => io.observe(el));
}

/* -------------------------------------------------------------------------- */
/* Pointer-reactive 3D tilt + specular glare on the hero screenshot           */
/* -------------------------------------------------------------------------- */
function initTilt(): void {
  const tilt = document.querySelector<HTMLElement>(".hero-tilt");
  const shot = tilt?.querySelector<HTMLElement>(".hero-shot");
  if (!tilt || !shot || !finePointer() || reduced()) return;

  const MAX_X = 6; // rotateX magnitude (deg)
  const MAX_Y = 8; // rotateY magnitude (deg)
  let raf = 0;
  let rx = 0;
  let ry = 0;
  let gx = 50;
  let gy = 50;
  let go = 0;

  const apply = (): void => {
    raf = 0;
    shot.style.setProperty("--rx", `${rx.toFixed(2)}deg`);
    shot.style.setProperty("--ry", `${ry.toFixed(2)}deg`);
    shot.style.setProperty("--gx", `${gx.toFixed(1)}%`);
    shot.style.setProperty("--gy", `${gy.toFixed(1)}%`);
    shot.style.setProperty("--go", go.toFixed(3));
  };
  const schedule = (): void => {
    if (!raf) raf = requestAnimationFrame(apply);
  };

  tilt.addEventListener("pointermove", (e) => {
    const r = tilt.getBoundingClientRect();
    const px = (e.clientX - r.left) / r.width; // 0..1
    const py = (e.clientY - r.top) / r.height; // 0..1
    ry = (px - 0.5) * 2 * MAX_Y;
    rx = -(py - 0.5) * 2 * MAX_X;
    gx = px * 100;
    gy = py * 100;
    go = 0.32;
    tilt.classList.add("is-tilting");
    schedule();
  });

  tilt.addEventListener("pointerleave", () => {
    rx = 0;
    ry = 0;
    go = 0;
    gx = 50;
    gy = 50;
    tilt.classList.remove("is-tilting");
    schedule();
  });
}

/* -------------------------------------------------------------------------- */
/* Cursor-follow spotlight on cards (radial light tracks the pointer)         */
/* -------------------------------------------------------------------------- */
function initCardSpotlight(): void {
  if (!finePointer() || reduced()) return;
  const cards = Array.from(document.querySelectorAll<HTMLElement>(".card"));
  for (const card of cards) {
    card.addEventListener("pointermove", (e) => {
      const r = card.getBoundingClientRect();
      card.style.setProperty("--mx", `${((e.clientX - r.left) / r.width) * 100}%`);
      card.style.setProperty("--my", `${((e.clientY - r.top) / r.height) * 100}%`);
    });
  }
}

/* -------------------------------------------------------------------------- */
/* Global trailing cursor glow (lerped — soft light follows the pointer)      */
/* -------------------------------------------------------------------------- */
function initCursorGlow(): void {
  if (!finePointer() || reduced()) return;
  const glow = document.querySelector<HTMLElement>(".cursor-glow");
  if (!glow) return;

  let tx = window.innerWidth / 2;
  let ty = window.innerHeight / 2;
  let x = tx;
  let y = ty;
  let raf = 0;

  const loop = (): void => {
    x += (tx - x) * 0.16;
    y += (ty - y) * 0.16;
    glow.style.transform = `translate3d(${x.toFixed(1)}px, ${y.toFixed(1)}px, 0)`;
    // Stop the loop once we've effectively caught up to the pointer.
    if (Math.abs(tx - x) < 0.4 && Math.abs(ty - y) < 0.4) {
      raf = 0;
      return;
    }
    raf = requestAnimationFrame(loop);
  };
  const kick = (): void => {
    if (!raf) raf = requestAnimationFrame(loop);
  };

  window.addEventListener(
    "pointermove",
    (e) => {
      tx = e.clientX;
      ty = e.clientY;
      glow.classList.add("is-on");
      kick();
    },
    { passive: true }
  );
  window.addEventListener("pointerdown", () => glow.classList.add("is-press"));
  window.addEventListener("pointerup", () => glow.classList.remove("is-press"));
  document.addEventListener("pointerleave", () => glow.classList.remove("is-on"));
}

/* -------------------------------------------------------------------------- */
/* Roadmap spine "draws" downward when the timeline scrolls into view         */
/* -------------------------------------------------------------------------- */
function initTimelineDraw(): void {
  const tl = document.querySelector<HTMLElement>(".timeline");
  if (!tl || !("IntersectionObserver" in window)) return;
  const io = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        tl.classList.add("is-drawn");
        io.unobserve(e.target);
      }
    },
    { threshold: 0.12 }
  );
  io.observe(tl);
}

/* -------------------------------------------------------------------------- */
function init(): void {
  initReveals();
  initScrollProgress();
  initCounters();
  initTilt();
  initCardSpotlight();
  initCursorGlow();
  initTimelineDraw();
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", init, { once: true });
} else {
  init();
}
