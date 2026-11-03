import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

const host = process.env.TAURI_DEV_HOST

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: host || false,
    // catalog 放在仓库根 i18n/（前后端共用一份），dev server 需要放行上一级
    fs: { allow: ['..', '.'] },
    hmr: host ? { protocol: 'ws', host, port: 5174 } : undefined,
    watch: { ignored: ['**/src-tauri/**'] },
  },
})
