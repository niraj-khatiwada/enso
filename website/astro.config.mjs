// @ts-check

import mdx from '@astrojs/mdx'
import sitemap from '@astrojs/sitemap'
import tailwindcss from '@tailwindcss/vite'
import { defineConfig } from 'astro/config'

export default defineConfig({
  base: '/',
  site: 'https://enso.codeforreal.com',
  integrations: [mdx(), sitemap()],
  output: 'static',
  devToolbar: { enabled: false },
  outDir: 'dist',
  server: {
    allowedHosts: ['.trycloudflare.com'],
  },
  vite: {
    plugins: [tailwindcss()],
  },
})
