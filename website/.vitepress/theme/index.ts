import { h } from 'vue'
import DefaultTheme from 'vitepress/theme'
import type { Theme } from 'vitepress'
import BrandEmbed from './components/BrandEmbed.vue'
import HeroFilm from './components/HeroFilm.vue'
import HowItWorks from './components/HowItWorks.vue'
import MermaidDiagram from './components/MermaidDiagram.vue'
import Tabs from './components/Tabs.vue'
import './custom.css'

export default {
  extends: DefaultTheme,
  // The home page's film, full width under the hero's words (it renders only
  // where the page's frontmatter sets `film`).
  Layout: () => h(DefaultTheme.Layout, null, { 'home-hero-after': () => h(HeroFilm) }),
  enhanceApp({ app }) {
    app.component('BrandEmbed', BrandEmbed)
    app.component('HowItWorks', HowItWorks)
    app.component('MermaidDiagram', MermaidDiagram)
    app.component('Tabs', Tabs)
  }
} satisfies Theme
