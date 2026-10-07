<script setup lang="ts">
// BRAND SLOT. Embeds one of the brand kit's videos once its file is in website/public/:
//
//   kind="explainer"    public/media/explainer.mp4    (poster media/explainer-poster.png, captions media/explainer.vtt;
//                       its caption links the interactive explainer, public/explainer/index.html, when it is there)
//   kind="walkthrough"  public/media/walkthrough.mp4  (poster media/walkthrough-poster.png, captions media/walkthrough.vtt)
//   kind="math"         public/media/math.mp4         (poster media/math-poster.png, captions media/math.vtt)
//   kind="console"      public/media/console.mp4      (poster media/console-poster.png, captions media/console.vtt)
//
// config.mts checks for the files when the site builds. Until they are there, a
// build renders nothing here and `npm run dev` shows where the file goes. Each
// video plays in a 16:9 frame; embed each once per page.
import { computed } from 'vue'
import { useData, withBase } from 'vitepress'

/** Set by config.mts: each brand file's site path, or false while it is missing. */
type BrandAssets = Partial<
  Record<
    | 'explainerVideo'
    | 'explainerPoster'
    | 'explainerCaptions'
    | 'explainerPage'
    | 'walkthroughVideo'
    | 'walkthroughPoster'
    | 'walkthroughCaptions'
    | 'mathVideo'
    | 'mathPoster'
    | 'mathCaptions'
    | 'consoleVideo'
    | 'consolePoster'
    | 'consoleCaptions',
    string | false
  >
>

const props = withDefaults(
  defineProps<{
    kind: 'explainer' | 'walkthrough' | 'math' | 'console'
    title?: string
    caption?: string
    /** Width over height of the video's frame. */
    aspect?: string
  }>(),
  { aspect: '16 / 9' }
)

const { theme } = useData()
const assets = computed(() => (theme.value as { brandAssets?: BrandAssets }).brandAssets)

const expected = {
  explainer: 'media/explainer.mp4',
  walkthrough: 'media/walkthrough.mp4',
  math: 'media/math.mp4',
  console: 'media/console.mp4'
}

const file = (key: 'Video' | 'Poster' | 'Captions') => {
  const a = assets.value
  const path = a?.[`${props.kind}${key}` as keyof BrandAssets]
  return path ? withBase(path) : undefined
}

const src = computed(() => file('Video') ?? false)
const poster = computed(() => file('Poster'))
/** The voice-over's captions, WebVTT, off until the viewer turns them on (captionsOff). */
const captions = computed(() => file('Captions'))
/** The interactive explainer, a static page outside the router (so the link sets target). */
const page = computed(() => (props.kind === 'explainer' && assets.value?.explainerPage ? withBase('/explainer/') : undefined))

const label = computed(
  () =>
    props.title ??
    {
      explainer: 'How stretto works: the explainer video',
      walkthrough: 'The walkthrough, recorded',
      math: 'The math and the probabilistic programs inside stretto',
      console: 'A tour of the console'
    }[props.kind]
)

const isDev = import.meta.env.DEV
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
  <figure v-if="src" :id="`film-${kind}`" class="brand-embed" :class="`brand-embed--${kind}`">
    <div class="brand-embed__frame" :style="{ aspectRatio: aspect }">
      <video :src="src" :poster="poster" :aria-label="label" controls playsinline preload="none" @loadedmetadata="captionsOff">
        <track v-if="captions" kind="captions" :src="captions" srclang="en" label="English" />
      </video>
    </div>
    <figcaption v-if="caption || page" class="brand-embed__caption">
      {{ caption }}
      <a v-if="page" :href="page" target="_self">Step through it at your own pace.</a>
    </figcaption>
  </figure>
  <p v-else-if="isDev" class="brand-embed--missing">
    Brand slot ({{ kind }}): copy the brand kit's file to <code>website/public/{{ expected[kind] }}</code> to show it here.
  </p>
</template>

<style scoped>
.brand-embed {
  margin: 24px 0;
}

.brand-embed__frame {
  position: relative;
  width: 100%;
  overflow: hidden;
  border: 1px solid var(--vp-c-divider);
  border-radius: 12px;
  background: var(--vp-c-bg-soft);
}

.brand-embed__frame video {
  display: block;
  width: 100%;
  border: 0;
}

.brand-embed__frame video {
  height: 100%;
  object-fit: cover;
  background: #000;
}

.brand-embed__caption {
  margin-top: 8px;
  font-size: 14px;
  color: var(--vp-c-text-2);
  text-align: center;
}

.brand-embed--missing {
  padding: 12px 16px;
  border: 1px dashed var(--vp-c-divider);
  border-radius: 8px;
  font-size: 14px;
  color: var(--vp-c-text-3);
}
</style>
