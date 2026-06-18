// @ts-check
import { defineConfig } from 'astro/config';

// Deployed to GitHub Pages at https://rhask87062.github.io/alterm/
// `base` makes every asset/link resolve under the /alterm/ sub-path.
// If a custom domain is added later, set base back to '/'.
export default defineConfig({
  site: 'https://rhask87062.github.io',
  base: '/alterm/',
  trailingSlash: 'ignore',
  build: {
    inlineStylesheets: 'auto',
  },
});
