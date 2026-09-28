<script setup lang="ts">
/** Markdown from the server, rendered with raw HTML escaped (see lib/markdown). */
import { computed } from 'vue'
import { renderMarkdown } from '@/lib/markdown'

const props = defineProps<{ text: string }>()
const html = computed(() => renderMarkdown(props.text))
</script>

<template>
  <!-- eslint-disable-next-line vue/no-v-html -- markdown-it escapes raw HTML (html: false) -->
  <div class="md" v-html="html" />
</template>
