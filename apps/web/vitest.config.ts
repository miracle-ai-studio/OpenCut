import {defineConfig, mergeConfig} from 'vitest/config'
import viteConfig from './vite.config.ts'

export default mergeConfig(viteConfig, defineConfig({
  server: { fs: { allow: [new URL("../../", import.meta.url).pathname] } },
  test: { environment: 'node', include: ['src/**/*.test.ts', 'src/**/*.test.tsx'] },
}))
