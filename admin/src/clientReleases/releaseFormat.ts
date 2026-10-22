import { CLIENT_PLATFORMS, packageExtension, type ClientArch, type ClientPlatform } from './clientReleaseApi';

/** Pre-fills the form from conventional artifact names such as `TJXY-1.2.0-setup-x64.exe`. */
export function guessTarget(fileName: string): { platform?: ClientPlatform; arch?: ClientArch; version?: string } {
  const lower = fileName.toLowerCase();
  const platform = CLIENT_PLATFORMS.find((item) => packageExtension(item, fileName) !== null);
  const arch: ClientArch | undefined = platform === 'android' ? 'universal' : /arm64|aarch64/u.test(lower) ? 'arm64' : /x64|amd64|x86_64/u.test(lower) ? 'x64' : undefined;
  const version = /\d+\.\d+\.\d+/u.exec(fileName)?.[0];
  return { ...(platform === undefined ? {} : { platform }), ...(arch === undefined ? {} : { arch }), ...(version === undefined ? {} : { version }) };
}

export function formatBytes(value: number) { const units = ['B', 'KiB', 'MiB', 'GiB']; let size = value; let unit = 0; while (size >= 1024 && unit < units.length - 1) { size /= 1024; unit += 1; } return `${unit === 0 ? String(size) : size.toFixed(1)} ${units[unit] ?? 'B'}`; }
