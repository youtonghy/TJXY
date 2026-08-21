const rawBuildVersion = (import.meta.env as Record<string, unknown>).VITE_TJXY_VERSION;

export const BUILD_VERSION = typeof rawBuildVersion === 'string' && rawBuildVersion.trim() !== ''
  ? rawBuildVersion.trim()
  : '0.0.0';
