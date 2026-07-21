import react from '@vitejs/plugin-react';
import { defineConfig, loadEnv } from 'vite';

export default defineConfig(({ mode }) => {
  const environment = loadEnv(mode, process.cwd(), 'TJXY_');
  const target = environment.TJXY_DEV_SERVER ?? 'http://127.0.0.1:8096';

  return {
    base: '/admin/',
    plugins: [react()],
    build: {
      outDir: 'dist',
      emptyOutDir: true,
    },
    server: {
      proxy: {
        '/Users': target,
        '/System': target,
        '/health': target,
      },
    },
  };
});
