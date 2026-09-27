import DefaultTheme from 'vitepress/theme'
import type { Theme } from 'vitepress'
import BrandEmbed from './components/BrandEmbed.vue'
import HowItWorks from './components/HowItWorks.vue'
import MermaidDiagram from './components/MermaidDiagram.vue'
import Tabs from './components/Tabs.vue'
import './custom.css'

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    app.component('BrandEmbed', BrandEmbed)
    app.component('HowItWorks', HowItWorks)
    app.component('MermaidDiagram', MermaidDiagram)
    app.component('Tabs', Tabs)
  }
} satisfies Theme
