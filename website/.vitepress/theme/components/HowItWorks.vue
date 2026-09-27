<script setup lang="ts">
// The home page's "how it works" strip: the loop, in four steps.
import { withBase } from 'vitepress'

const steps = [
  {
    title: 'Record',
    text: 'stretto init prints your MCP host’s config, with stretto-proxy in place of the server’s command. The proxy forwards every message and logs each session.',
    code: 'stretto init --host cursor --domain orders -- <server>',
    link: '/guide/concepts/sessions'
  },
  {
    title: 'Learn',
    text: 'stretto learn counts which reads followed which calls, and where each argument came from. Learning is counting: no model, no key.',
    code: 'stretto learn --sessions ~/.stretto/logs/orders --habit-only …',
    link: '/guide/concepts/flows'
  },
  {
    title: 'Review',
    text: 'A flow is a file. flow-show renders it for a reviewer, flow-diff lists what changed, and shadow mode runs it without letting it act.',
    code: 'stretto flow-show orders.flow.json',
    link: '/guide/concepts/audit-and-review'
  },
  {
    title: 'Serve',
    text: 'After each of the agent’s calls, the flow’s lookups ride in the same tool result, so the agent skips the turns it would have spent asking.',
    code: 'stretto init --host cursor --flow orders.flow.json -- <server>',
    link: '/guide/concepts/lookups'
  }
]
</script>

<template>
  <div class="how-it-works-frame">
    <ol class="how-it-works">
      <li v-for="(step, index) in steps" :key="step.title" class="how-it-works__step">
        <a class="how-it-works__card" :href="withBase(step.link)">
          <span class="how-it-works__number" aria-hidden="true">{{ index + 1 }}</span>
          <span class="how-it-works__title">{{ step.title }}</span>
          <span class="how-it-works__text">{{ step.text }}</span>
          <code class="how-it-works__code"><template v-for="(word, i) in step.code.split(' ')" :key="i">{{ i ? ' ' : '' }}<span class="how-it-works__word">{{ word }}</span></template></code>
        </a>
      </li>
    </ol>
  </div>
</template>

<style scoped>
.how-it-works-frame {
  container-type: inline-size;
  margin: 24px 0 8px;
}

.how-it-works {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 16px;
  margin: 0;
  padding: 0;
  list-style: none;
  counter-reset: none;
}

.how-it-works__step {
  position: relative;
  margin: 0;
}

.how-it-works__step + .how-it-works__step::before {
  content: '';
  position: absolute;
  top: 34px;
  left: -16px;
  width: 16px;
  height: 2px;
  background: var(--vp-c-divider);
}

.how-it-works__card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  height: 100%;
  padding: 20px;
  border: 1px solid var(--vp-c-bg-soft);
  border-radius: 12px;
  background: var(--vp-c-bg-soft);
  color: inherit;
  text-decoration: none;
  transition: border-color 0.25s, background-color 0.25s;
}

.how-it-works__card:hover {
  border-color: var(--vp-c-brand-1);
}

.how-it-works__number {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 50%;
  font-size: 14px;
  font-weight: 600;
  color: var(--vp-c-brand-1);
  background: var(--vp-c-brand-soft);
}

.how-it-works__title {
  font-size: 16px;
  font-weight: 600;
  line-height: 24px;
  color: var(--vp-c-text-1);
}

.how-it-works__text {
  flex-grow: 1;
  font-size: 14px;
  line-height: 22px;
  color: var(--vp-c-text-2);
}

.how-it-works__code {
  display: block;
  padding: 6px 8px;
  border-radius: 6px;
  font-family: var(--vp-font-family-mono);
  font-size: 12px;
  line-height: 18px;
  color: var(--vp-c-text-1);
  background: var(--vp-c-bg);
  overflow-wrap: anywhere;
}

/* A command breaks between its words, not after a flag's hyphens; only a
   word wider than the box breaks inside itself. */
.how-it-works__word {
  display: inline-block;
  max-width: 100%;
}

/* Four steps side by side need the home page's width. In a page's text
   column, their commands would break inside words: two by two there. */
@container (max-width: 879px) {
  .how-it-works {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .how-it-works__step:nth-child(odd)::before {
    display: none;
  }
}

@media (max-width: 959px) {
  .how-it-works {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .how-it-works__step:nth-child(odd)::before {
    display: none;
  }
}

@media (max-width: 639px) {
  .how-it-works {
    grid-template-columns: minmax(0, 1fr);
  }

  .how-it-works__step + .how-it-works__step::before {
    display: none;
  }
}
</style>
