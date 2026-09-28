/**
 * `npm run dev:mock` (vite --mode mock): this plugin answers /api/* from the
 * mock world in ./fixtures, so the UI runs and is tested without the server.
 */
import type { Plugin } from 'vite'
import { handle, MOCK_TOKEN } from './api.ts'

export function mockApi(): Plugin {
  return {
    name: 'stretto-console-mock-api',
    configureServer(server) {
      server.middlewares.use((req, res, next) => void handle(req, res, next))
      server.httpServer?.once('listening', () => {
        const auth = process.env.MOCK_AUTH === '1' ? ` (auth on: the token is ${MOCK_TOKEN})` : ''
        server.config.logger.info(`  stretto console: mock API on /api${auth}`)
      })
    },
    configurePreviewServer(server) {
      server.middlewares.use((req, res, next) => void handle(req, res, next))
    },
  }
}
