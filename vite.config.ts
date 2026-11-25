import {defineConfig} from 'vite';

export default defineConfig({
  clearScreen: false,
  server: {port: 1420, strictPort: true},
  build: {target: 'es2022', outDir: (process.env.TAURI_ENV_PLATFORM || process.platform) === 'linux' ? 'dist-linux' : 'dist'},
  plugins: [{
    name: 'platform-viewer',
    transformIndexHtml: {
      order: 'pre',
      handler: html => (process.env.TAURI_ENV_PLATFORM || process.platform) === 'linux'
        ? html.replace('/src/main.ts', '/src/main.linux.ts') : html
    }
  }]
});
