import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/postcss';
import { cp, mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
const root = path.dirname(fileURLToPath(import.meta.url));
export default defineConfig({
  resolve: { alias: { '@': root } },
  css: { postcss: { plugins: [tailwindcss()] } },
  publicDir: false,
  build: { outDir: 'dist/selfhost', emptyOutDir: true },
  server: {
    host: '127.0.0.1',
    proxy: {
      '/api': 'http://127.0.0.1:8080',
      '/example': 'http://127.0.0.1:8080',
      '/favicon.svg': 'http://127.0.0.1:8080',
    },
  },
  plugins: [
    react(),
    {
      name: 'tempo-static-assets',
      async closeBundle() {
        await mkdir(path.join(root, 'dist/selfhost/example'), { recursive: true });
        await cp(
          path.join(root, 'public/example'),
          path.join(root, 'dist/selfhost/example'),
          { recursive: true },
        );
        await cp(
          path.join(root, 'public/favicon.svg'),
          path.join(root, 'dist/selfhost/favicon.svg'),
        );
      },
    },
  ],
});
