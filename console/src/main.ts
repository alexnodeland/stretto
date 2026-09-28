import { createApp } from 'vue'
import App from './App.vue'
import { makeRouter } from './router'
import { applyTheme, theme } from './stores/theme'
import './styles/fonts.css'
import './styles/tokens.css'
import './styles/base.css'

applyTheme(theme.value)

createApp(App).use(makeRouter()).mount('#app')
