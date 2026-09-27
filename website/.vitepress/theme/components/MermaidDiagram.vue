<script setup lang="ts">
// A ```mermaid block (see config.mts), drawn in the browser. Until Mermaid has
// loaded, and without JavaScript, the diagram's source shows instead.
import { onMounted, ref, watch } from 'vue'
import { useData } from 'vitepress'

const props = defineProps<{ code: string }>()
const source = decodeURIComponent(props.code)
const { isDark } = useData()
const svg = ref('')
let renders = 0

async function draw() {
  const id = `mermaid-${Math.random().toString(36).slice(2)}-${renders++}`
  try {
    const { default: mermaid } = await import('mermaid')
    const style = getComputedStyle(document.documentElement)
    const color = (name: string) => style.getPropertyValue(name).trim()
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: 'strict',
      // SVG text, not HTML: the page's styles would change HTML labels' line height
      // after Mermaid measured them, and clip a second line.
      htmlLabels: false,
      flowchart: { htmlLabels: false },
      sequence: { mirrorActors: false, actorMargin: 40, messageFontSize: 15, noteFontSize: 14, actorFontSize: 15 },
      theme: 'base',
      fontFamily: color('--vp-font-family-base'),
      themeVariables: {
        darkMode: isDark.value,
        background: color('--vp-c-bg'),
        primaryColor: color('--vp-c-bg-soft'),
        primaryTextColor: color('--vp-c-text-1'),
        primaryBorderColor: color('--vp-c-brand-2'),
        secondaryColor: color('--vp-c-bg-alt'),
        tertiaryColor: color('--vp-c-bg-alt'),
        lineColor: color('--vp-c-text-2'),
        textColor: color('--vp-c-text-1'),
        clusterBkg: color('--vp-c-bg-alt'),
        clusterBorder: color('--vp-c-divider'),
        noteBkgColor: color('--vp-c-brand-soft'),
        noteTextColor: color('--vp-c-text-1'),
        noteBorderColor: color('--vp-c-brand-2'),
        actorBkg: color('--vp-c-bg-soft'),
        actorBorder: color('--vp-c-brand-2'),
        actorTextColor: color('--vp-c-text-1'),
        signalColor: color('--vp-c-text-2'),
        signalTextColor: color('--vp-c-text-1'),
        edgeLabelBackground: color('--vp-c-bg'),
        fontSize: '14px'
      }
    })
    const { svg: drawn } = await mermaid.render(id, source)
    svg.value = drawn
  } catch (error) {
    // Leave the source showing.
    document.getElementById(id)?.remove()
    console.error('Could not draw the diagram', error)
  }
}

onMounted(draw)
watch(isDark, draw)
</script>

<template>
  <div class="mermaid-diagram">
    <div v-if="svg" class="mermaid-diagram__svg" v-html="svg" />
    <pre v-else class="mermaid-diagram__source"><code>{{ source }}</code></pre>
  </div>
</template>

<style scoped>
.mermaid-diagram {
  margin: 16px 0;
  overflow-x: auto;
}

.mermaid-diagram__svg {
  display: flex;
  justify-content: center;
  min-width: min-content;
}

.mermaid-diagram__svg :deep(svg) {
  max-width: 100%;
  height: auto;
}

.mermaid-diagram__source {
  margin: 0;
  padding: 16px;
  border-radius: 8px;
  background: var(--vp-code-block-bg);
  font-family: var(--vp-font-family-mono);
  font-size: 13px;
  line-height: 1.6;
  overflow-x: auto;
}
</style>
