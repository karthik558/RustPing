import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

export default defineConfig({
  plugins: [vue()],
  base: '/static/app/',
  publicDir: 'logo',
  build: {
    outDir: 'static/app',
    emptyOutDir: true,
  },
})
