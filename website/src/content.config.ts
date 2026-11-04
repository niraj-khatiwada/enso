import { glob } from 'astro/loaders'
import { defineCollection } from 'astro:content'

/** The app's CHANGELOG.md (at the repository's root), read whenever the site is built */
const changelog = defineCollection({
  loader: glob({ base: '..', pattern: 'CHANGELOG.md' }),
})

export const collections = { changelog }
