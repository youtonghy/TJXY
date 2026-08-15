import react from '@vitejs/plugin-react';
import { defineConfig, loadEnv } from 'vite';

export default defineConfig(({ mode }) => {
  const environment = loadEnv(mode, process.cwd(), 'TJXY_');
  const target = environment.TJXY_DEV_SERVER ?? 'http://127.0.0.1:8096';

  return {
    base: '/',
    plugins: [react()],
    build: {
      outDir: 'dist',
      emptyOutDir: true,
      rolldownOptions: {
        output: {
          codeSplitting: {
            groups: [
              { name: 'charts', test: /node_modules[\\/](recharts|victory-vendor|@reduxjs[\\/]toolkit|react-redux|redux|reselect|immer|decimal\.js-light|es-toolkit|eventemitter3|react-smooth)[\\/]/ },
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
        '/Ai': target,
        '/Auth': target,
        '/Devices': target,
        '/Library': target,
        '/ScheduledTasks': target,
        '/Users': target,
        '/System': target,
        '/health': target,
        '/Items': target,
        '/UserItems': target,
        '/Shows': target,
        '/Search': target,
        '/Setup': target,
        '/UserViews': target,
        '/Sessions': target,
        '/Videos': target,
        '/Audio': target,
        '/Discover': target,
        '/PlaybackTickets': target,
      },
    },
  };
});
