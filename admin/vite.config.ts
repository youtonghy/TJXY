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
      rolldownOptions: {
        output: {
          codeSplitting: {
            groups: [
              { name: 'heroui', test: /node_modules[\\/](@heroui|react-aria|@react-aria|tailwind-variants)[\\/]/ },
              { name: 'ra-core', test: /node_modules[\\/](ra-core|@tanstack|react-hook-form)[\\/]/ },
              { name: 'react', test: /node_modules[\\/](react|react-dom|react-router)[\\/]/ },
            ],
          },
        },
      },
    },
    server: {
      proxy: {
        '/Admin': target,
        '/Auth': target,
        '/Devices': target,
        '/Library': target,
        '/ScheduledTasks': target,
        '/Users': target,
        '/System': target,
        '/health': target,
      },
    },
  };
});
