// Copies the brand kit's explainer, its narration, and the videos with their
// captions (brand/) into public/, so that the site serves them at
// /stretto/explainer/ and /stretto/media/ in development and in the build. The
// copies are ignored by git: brand/ stays the only source.
// The logo, the icons and the social card are small, and are committed in public/.
import { copyFileSync, existsSync, mkdirSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const brand = fileURLToPath(new URL('../../brand/', import.meta.url))
const pub = fileURLToPath(new URL('../public/', import.meta.url))
const files = [
  ['explainer/index.html', 'explainer/index.html'],
  ['media/launch.mp4', 'media/launch.mp4'],
  ['media/launch-poster.png', 'media/launch-poster.png'],
  ['media/launch-teaser.webm', 'media/launch-teaser.webm'],
  ['media/walkthrough.mp4', 'media/walkthrough.mp4'],
  ['media/walkthrough-poster.png', 'media/walkthrough-poster.png'],
  ['media/launch.vtt', 'media/launch.vtt'],
  ['media/walkthrough.vtt', 'media/walkthrough.vtt'],
  // The explainer's narration, next to it (brand/video/narrate.py).
  ...Array.from({ length: 7 }, (_, i) => [`explainer/audio/step-${i + 1}.mp3`, `explainer/audio/step-${i + 1}.mp3`])
]

let copied = 0
for (const [from, to] of files) {
  if (!existsSync(path.join(brand, from))) {
    console.warn(`brand/${from} is missing; its slot on the site renders nothing`)
    continue
  }
  mkdirSync(path.dirname(path.join(pub, to)), { recursive: true })
  copyFileSync(path.join(brand, from), path.join(pub, to))
  copied++
}
console.log(`Copied ${copied} of ${files.length} brand files into website/public/`)
