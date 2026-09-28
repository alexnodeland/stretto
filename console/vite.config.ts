import { fileURLToPath, URL } from 'node:url'
import { defineConfig, searchForWorkspaceRoot, type PluginOption } from 'vite'
import vue from '@vitejs/plugin-vue'
import { mockApi } from './mock/plugin.ts'

const brand = fileURLToPath(new URL('../brand', import.meta.url))

// The console server `npm run dev` and `vite preview` pass /api on to
// (stretto-console's default listen address). The Host is rewritten to the
// server's own, which a server started with --no-auth insists on.
const server = process.env.STRETTO_CONSOLE_URL ?? 'http://127.0.0.1:7878'
const proxy = {
  '/api': { target: server, changeOrigin: true },
  // `?token=` on any page: the server sets its cookie and redirects without it.
  '^/[^?]*\\?(.*&)?token=': { target: server, changeOrigin: true },
}

// The server's Content-Security-Policy, which `vite preview` holds the build to.
const CSP =
  "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; connect-src 'self'"

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
      proxy: mock ? undefined : proxy,
    },
    preview: {
      host: '127.0.0.1',
      port: 4173,
      proxy: mock ? undefined : proxy,
      headers: { 'Content-Security-Policy': CSP },
    },
    build: {
      outDir: 'dist',
      emptyOutDir: true,
      // The server's CSP allows fonts from 'self' only, never data: URIs.
      assetsInlineLimit: (file: string) => (/\.woff2?$/.test(file) ? false : undefined),
      chunkSizeWarningLimit: 900,
    },
  }
})
