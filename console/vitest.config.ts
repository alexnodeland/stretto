import { defineConfig, mergeConfig } from 'vitest/config'
import viteConfig from './vite.config.ts'

export default defineConfig((env) =>
  mergeConfig(viteConfig(env), {
    test: {
      environment: 'happy-dom',
      include: ['tests/unit/**/*.spec.ts'],
      env: { TZ: 'UTC' },
      restoreMocks: true,
    },
  }),
)
