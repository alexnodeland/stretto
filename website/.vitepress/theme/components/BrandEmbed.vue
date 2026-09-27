<script setup lang="ts">
// BRAND SLOT. Embeds a piece of the brand kit once its file is in website/public/:
//
//   kind="explainer"    public/explainer/index.html   (an iframe)
//   kind="launch"       public/media/launch.mp4       (a video, poster media/launch-poster.png)
//   kind="walkthrough"  public/media/walkthrough.mp4  (a video, poster media/walkthrough-poster.png)
//
// config.mts checks for the files when the site builds. Until they are there, a
// build renders nothing here and `npm run dev` shows where the file goes.
//
// The videos play in a 16:9 frame. The explainer's frame is as tall as the
// explainer: it posts its height as a `stretto-explainer:height` message
// (brand/README.md, "Explainer").
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useData, withBase } from 'vitepress'

/** Set by config.mts: each brand file's site path, or false while it is missing. */
type BrandAssets = Partial<Record<'explainer' | 'launchVideo' | 'launchPoster' | 'walkthroughVideo' | 'walkthroughPoster', string | false>>

const props = withDefaults(
  defineProps<{
    kind: 'explainer' | 'launch' | 'walkthrough'
    title?: string
    caption?: string
    /** Width over height of a video's frame. */
    aspect?: string
  }>(),
  { aspect: '16 / 9' }
)

const { theme } = useData()
const assets = computed(() => (theme.value as { brandAssets?: BrandAssets }).brandAssets)

const expected = {
  explainer: 'explainer/index.html',
  launch: 'media/launch.mp4',
  walkthrough: 'media/walkthrough.mp4'
}

const src = computed(() => {
  const a = assets.value
  if (!a) return false
  if (props.kind === 'explainer') return a.explainer ? withBase('/explainer/') : false
  const video = props.kind === 'launch' ? a.launchVideo : a.walkthroughVideo
  return video ? withBase(video) : false
})

const poster = computed(() => {
  const a = assets.value
  if (!a || props.kind === 'explainer') return undefined
  const file = props.kind === 'launch' ? a.launchPoster : a.walkthroughPoster
  return file ? withBase(file) : undefined
})

const label = computed(
  () =>
    props.title ??
    {
      explainer: 'How stretto works, animated',
      launch: 'stretto in two minutes',
      walkthrough: 'The walkthrough, recorded'
    }[props.kind]
)

const isDev = import.meta.env.DEV

/**
 * The explainer's height in px, from its last message. It reports its page's
 * scrollHeight, which is never less than the frame's own height, so the frame
 * can grow to fit it but never shrink: start below its height on a wide page.
 */
const explainerHeight = ref(720)
const frame = ref<HTMLIFrameElement | null>(null)

function onMessage(e: MessageEvent) {
  if (!frame.value || e.source !== frame.value.contentWindow) return
  const d = e.data as { type?: unknown; height?: unknown } | null
  if (d?.type !== 'stretto-explainer:height') return
  const height = Number(d.height)
  if (Number.isFinite(height) && height > 0) explainerHeight.value = Math.ceil(height)
}

onMounted(() => {
  if (props.kind === 'explainer') window.addEventListener('message', onMessage)
})

onBeforeUnmount(() => window.removeEventListener('message', onMessage))
</script>

<template>
  <figure v-if="src" class="brand-embed" :class="`brand-embed--${kind}`">
    <div class="brand-embed__frame" :style="kind === 'explainer' ? undefined : { aspectRatio: aspect }">
      <iframe
        v-if="kind === 'explainer'"
        ref="frame"
        :src="src"
        :style="{ height: `${explainerHeight}px` }"
        :title="label"
        loading="lazy"
        allow="fullscreen"
      />
      <video v-else :src="src" :poster="poster" :aria-label="label" controls playsinline preload="none" />
    </div>
    <figcaption v-if="caption" class="brand-embed__caption">{{ caption }}</figcaption>
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

.brand-embed__frame iframe,
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
