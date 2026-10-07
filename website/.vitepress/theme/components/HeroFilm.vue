<script setup lang="ts">
// The home page's films: the explainer, full width, straight under the hero's
// words (the `home-hero-after` slot, set in theme/index.ts), and under it the
// two deep dives, each playing where it is listed. A poster shows with one
// play control; a click starts that video with its controls. The captions
// are in the picture, so the WebVTT track starts off (captionsOff).
// Renders nothing until the brand kit's files are in public/
// (scripts/copy-brand.mjs), as BrandEmbed does.
import { computed, ref } from 'vue'
import { useData, withBase } from 'vitepress'
import type { BrandAssets } from '../../config.mts'

const { theme, frontmatter } = useData()
const assets = computed(() => (theme.value as { brandAssets?: Partial<BrandAssets> }).brandAssets)
const film = computed(() => frontmatter.value.film as { label?: string } | undefined)

const at = (p: string | false | undefined) => (p ? withBase(p) : undefined)
const src = computed(() => at(assets.value?.explainerVideo))
const poster = computed(() => at(assets.value?.explainerPoster))
const captions = computed(() => at(assets.value?.explainerCaptions))
const page = computed(() => (assets.value?.explainerPage ? withBase('/explainer/') : undefined))
/** The deep dives, under the film: each once its file is in public/. */
const deeper = computed(() =>
  [
    { key: 'math', title: 'The math, and the probabilistic programs', text: 'The event stretto estimates, the rule that acts on it, the counts and alpha’s posterior in fugue, and how the estimate held up in replay and live.', href: '/research/paper', more: 'The paper' },
    { key: 'console', title: 'The console', text: 'Servers, sessions, flows and jobs, driven on the real console: a connection test, a session’s decisions, a threshold moved, an audit run from the page.', href: '/guide/console', more: 'The console guide' }
  ].flatMap(d => {
    const a = assets.value as Record<string, string | false> | undefined
    return a?.[`${d.key}Video`]
      ? [{ ...d, src: at(a[`${d.key}Video`] as string), poster: at(a[`${d.key}Poster`] as string | false), captions: at(a[`${d.key}Captions`] as string | false), href: withBase(d.href) }]
      : []
  })
)

const video = ref<HTMLVideoElement | null>(null)
const started = ref(false)
function play() {
  started.value = true
  const v = video.value
  if (!v) return
  v.controls = true
  void v.play()
}
/** A deep dive plays where it is listed, in place of its poster. */
const playing = ref<string | null>(null)
function playDive(key: string, el: HTMLVideoElement | null) {
  playing.value = key
  if (el) {
    el.controls = true
    void el.play()
  }
}
const diveVideos = ref<Record<string, HTMLVideoElement | null>>({})
/**
 * The films set their captions in the picture, so the WebVTT track starts
 * off even where the browser would show it by its own caption settings; the
 * player's captions control still turns it on.
 */
function captionsOff(e: Event) {
  for (const t of Array.from((e.target as HTMLVideoElement).textTracks)) t.mode = 'disabled'
}
</script>

<template>
  <section v-if="src && film" id="film" class="hero-film" aria-label="The films">
    <div class="hero-film__frame">
      <video
        ref="video"
        :src="src"
        :poster="poster"
        preload="none"
        playsinline
        @loadedmetadata="captionsOff"
        :controls="started"
        aria-label="How stretto works: the explainer video"
      >
        <track v-if="captions" kind="captions" :src="captions" srclang="en" label="English" />
      </video>
      <button v-if="!started" class="hero-film__play" type="button" @click="play">
        <span class="hero-film__icon" aria-hidden="true">
          <svg viewBox="0 0 24 24" width="28" height="28"><path d="M8 5.5v13l11-6.5z" fill="currentColor" /></svg>
        </span>
        <span class="hero-film__label">
          <strong>{{ film.label ?? 'Watch the explainer' }}</strong>
        </span>
      </button>
    </div>
    <p class="hero-film__caption">
      How stretto reads ahead, learns a flow and decides a lookup, with the live results and their scope.
      <a v-if="page" :href="page" target="_self">Or step through it at your own pace.</a>
    </p>
    <div v-if="deeper.length" class="hero-film__deeper" aria-label="Deep dives">
      <article v-for="d in deeper" :key="d.key" class="hero-film__dive">
        <div class="hero-film__dive-frame">
          <video
            :ref="el => (diveVideos[d.key] = el as HTMLVideoElement | null)"
            :src="d.src"
            :poster="d.poster"
            preload="none"
            playsinline
            @loadedmetadata="captionsOff"
            :controls="playing === d.key"
            :aria-label="`Deep dive: ${d.title}`"
          >
            <track v-if="d.captions" kind="captions" :src="d.captions" srclang="en" label="English" />
          </video>
          <button v-if="playing !== d.key" class="hero-film__dive-play" type="button" :aria-label="`Play the deep dive: ${d.title}`" @click="playDive(d.key, diveVideos[d.key])">
            <span class="hero-film__icon hero-film__icon--small" aria-hidden="true">
              <svg viewBox="0 0 24 24" width="22" height="22"><path d="M8 5.5v13l11-6.5z" fill="currentColor" /></svg>
            </span>
          </button>
        </div>
        <div class="hero-film__dive-text">
          <span class="hero-film__dive-kicker">Deep dive</span>
          <strong>{{ d.title }}</strong>
          <span>{{ d.text }}</span>
          <a :href="d.href">{{ d.more }} →</a>
        </div>
      </article>
    </div>
  </section>
</template>

<style scoped>
.hero-film {
  margin: 0 auto 48px;
  padding: 8px 24px 0;
  max-width: 1152px;
}
@media (min-width: 640px) {
  .hero-film {
    padding: 8px 48px 0;
  }
}
@media (min-width: 960px) {
  .hero-film {
    padding: 0 64px;
  }
}

.hero-film__frame {
  position: relative;
  aspect-ratio: 16 / 9;
  overflow: hidden;
  border: 1px solid var(--vp-c-divider);
  border-radius: 14px;
  background: #0b0f11;
  box-shadow: 0 30px 60px -30px rgba(2, 118, 123, 0.35), 0 12px 24px -12px rgba(0, 0, 0, 0.35);
}

.hero-film__frame video {
  display: block;
  width: 100%;
  height: 100%;
  object-fit: cover;
  background: #0b0f11;
}

.hero-film__play {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 18px;
  width: 100%;
  border: 0;
  cursor: pointer;
  color: #eceff0;
  background: radial-gradient(ellipse 42% 34% at 50% 52%, rgba(11, 15, 17, 0.82), rgba(11, 15, 17, 0.35) 100%);
  transition: background 0.2s;
}
.hero-film__play:hover {
  background: radial-gradient(ellipse 42% 34% at 50% 52%, rgba(11, 15, 17, 0.72), rgba(11, 15, 17, 0.2) 100%);
}
.hero-film__play:focus-visible {
  outline: 3px solid var(--vp-c-brand-1);
  outline-offset: -3px;
}

.hero-film__icon {
  display: grid;
  place-items: center;
  width: 76px;
  height: 76px;
  padding-left: 4px;
  border-radius: 50%;
  color: #071114;
  background: #33c0c7;
  box-shadow: 0 0 0 10px rgba(51, 192, 199, 0.18);
  transition: transform 0.2s;
}
.hero-film__play:hover .hero-film__icon {
  transform: scale(1.06);
}

.hero-film__label {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  text-align: left;
  line-height: 1.3;
}
.hero-film__label strong {
  font-size: 22px;
  font-weight: 600;
  letter-spacing: -0.01em;
}
@media (max-width: 520px) {
  .hero-film__icon {
    width: 56px;
    height: 56px;
  }
  .hero-film__label strong {
    font-size: 17px;
  }
}

.hero-film__caption {
  margin: 12px 0 0;
  font-size: 14px;
  color: var(--vp-c-text-2);
  text-align: center;
}
.hero-film__deeper {
  display: grid;
  grid-template-columns: 1fr;
  gap: 24px;
  margin-top: 32px;
}
@media (min-width: 768px) {
  .hero-film__deeper {
    grid-template-columns: 1fr 1fr;
  }
}
.hero-film__dive {
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.hero-film__dive-frame {
  position: relative;
  aspect-ratio: 16 / 9;
  overflow: hidden;
  border: 1px solid var(--vp-c-divider);
  border-radius: 12px;
  background: #0b0f11;
}
.hero-film__dive-frame video {
  display: block;
  width: 100%;
  height: 100%;
  object-fit: cover;
  background: #0b0f11;
}
.hero-film__dive-play {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  width: 100%;
  border: 0;
  cursor: pointer;
  background: rgba(11, 15, 17, 0.28);
  transition: background 0.2s;
}
.hero-film__dive-play:hover {
  background: rgba(11, 15, 17, 0.12);
}
.hero-film__dive-play:focus-visible {
  outline: 3px solid var(--vp-c-brand-1);
  outline-offset: -3px;
}
.hero-film__dive-play:hover .hero-film__icon {
  transform: scale(1.06);
}
.hero-film__icon--small {
  width: 56px;
  height: 56px;
  box-shadow: 0 0 0 8px rgba(51, 192, 199, 0.18);
}
.hero-film__dive-text {
  display: flex;
  flex-direction: column;
  gap: 4px;
  line-height: 1.45;
}
.hero-film__dive-kicker {
  font-family: var(--vp-font-family-mono);
  font-size: 12px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--vp-c-brand-1);
}
.hero-film__dive-text strong {
  font-size: 17px;
}
.hero-film__dive-text > span:last-of-type {
  font-size: 14px;
  color: var(--vp-c-text-2);
}
.hero-film__dive-text a {
  font-size: 14px;
  font-weight: 500;
  color: var(--vp-c-brand-1);
  text-decoration: none;
}
.hero-film__dive-text a:hover {
  text-decoration: underline;
}
</style>
