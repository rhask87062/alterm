// Platform detection for the download page.
//
// Plain JS rather than TypeScript so Node can run the tests in
// `scripts/check-detect-os.mjs` directly, with no build step.
//
// Returns null for anything we do not ship a desktop installer for — phones,
// tablets, ChromeOS, crawlers, unknown platforms. Null means "show everything
// and recommend nothing", which is also the no-JavaScript baseline. Guessing
// here is worse than not guessing: recommending a .deb to someone on an
// iPhone is actively unhelpful.

/**
 * @param {string} platform  navigator.platform (legacy, still the strongest signal)
 * @param {string} userAgent navigator.userAgent
 * @returns {"linux"|"macos"|"windows"|null}
 */
export function detectOs(platform, userAgent) {
  const probe = `${platform || ""} ${userAgent || ""}`;

  // Mobile and ChromeOS first: an Android UA contains "Linux", and iPadOS
  // reports "Mac OS X", so checking desktop families first would misfire.
  if (/Android|iPhone|iPad|iPod|CrOS|Windows Phone/i.test(probe)) return null;

  if (/Win/i.test(probe)) return "windows";
  if (/Mac/i.test(probe)) return "macos";
  if (/Linux|X11|BSD/i.test(probe)) return "linux";

  return null;
}
