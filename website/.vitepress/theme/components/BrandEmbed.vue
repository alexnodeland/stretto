<script setup lang="ts">
// BRAND SLOT. Embeds one of the brand kit's videos once its file is in website/public/:
//
//   kind="explainer"    public/media/explainer.mp4    (poster media/explainer-poster.png, captions media/explainer.vtt;
//                       its caption links the interactive explainer, public/explainer/index.html, when it is there)
//   kind="walkthrough"  public/media/walkthrough.mp4  (poster media/walkthrough-poster.png, captions media/walkthrough.vtt)
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
    | 'walkthroughCaptions',
    string | false
  >
>

const props = withDefaults(
  defineProps<{
    kind: 'explainer' | 'walkthrough'
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
  walkthrough: 'media/walkthrough.mp4'
}

const file = (key: 'Video' | 'Poster' | 'Captions') => {
  const a = assets.value
  const path = a?.[`${props.kind}${key}` as keyof BrandAssets]
  return path ? withBase(path) : undefined
}

const src = computed(() => file('Video') ?? false)
const poster = computed(() => file('Poster'))
/** The voice-over's captions, WebVTT, off until the viewer turns them on. */
const captions = computed(() => file('Captions'))
/** The interactive explainer, a static page outside the router (so the link sets target). */
const page = computed(() => (props.kind === 'explainer' && assets.value?.explainerPage ? withBase('/explainer/') : undefined))

const label = computed(
  () =>
    props.title ??
    {
      explainer: 'How stretto works, explained in under three minutes',
      walkthrough: 'The walkthrough, recorded'
    }[props.kind]
)

const isDev = import.meta.env.DEV
</script>

<template>
  <figure v-if="src" class="brand-embed" :class="`brand-embed--${kind}`">
    <div class="brand-embed__frame" :style="{ aspectRatio: aspect }">
      <video :src="src" :poster="poster" :aria-label="label" controls playsinline preload="none">
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
