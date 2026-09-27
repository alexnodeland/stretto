// Copies the research notebook (site/index.html) into public/notebook/, so that
// the site serves it at /stretto/notebook/ in development and in the build.
// The copy is ignored by git: site/index.html stays the only source.
import { copyFileSync, mkdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const from = fileURLToPath(new URL('../../site/index.html', import.meta.url))
const dir = fileURLToPath(new URL('../public/notebook/', import.meta.url))

mkdirSync(dir, { recursive: true })
copyFileSync(from, `${dir}index.html`)
console.log('Copied site/index.html to website/public/notebook/index.html')
