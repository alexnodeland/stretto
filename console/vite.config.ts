import { fileURLToPath, URL } from 'node:url'
import { defineConfig, searchForWorkspaceRoot, type PluginOption } from 'vite'
import vue from '@vitejs/plugin-vue'
import { mockApi } from './mock/plugin.ts'

const brand = fileURLToPath(new URL('../brand', import.meta.url))

// The console server `npm run dev` proxies to (stretto-console's default listen address).
const server = process.env.STRETTO_CONSOLE_URL ?? 'http://127.0.0.1:7878'

export default defineConfig(({ mode }) => {
  const mock = mode === 'mock'
  const plugins: PluginOption[] = [vue()]
  if (mock) plugins.push(mockApi())
  return {
    plugins,
    resolve: {
      alias: {
        '@': fileURLToPath(new URL('./src', import.meta.url)),
        // The brand kit is imported from ../brand, never copied: tokens, fonts and the logo.
        '@brand': brand,
      },
    },
    server: {
      host: '127.0.0.1',
      port: 5173,
      fs: { allow: [searchForWorkspaceRoot(process.cwd()), brand] },
      proxy: mock
        ? undefined
        : {
            '/api': { target: server },
            // `?token=` on any page: the server sets its cookie and redirects without it.
            '^/[^?]*\\?(.*&)?token=': { target: server },
          },
    },
    preview: { host: '127.0.0.1', port: 4173 },
    build: {
      outDir: 'dist',
      emptyOutDir: true,
      // The server's CSP allows fonts from 'self' only, never data: URIs.
      assetsInlineLimit: (file: string) => (/\.woff2?$/.test(file) ? false : undefined),
      chunkSizeWarningLimit: 900,
    },
  }
})
