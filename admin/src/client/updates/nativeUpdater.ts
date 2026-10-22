/**
 * Contract between the web client and the native shells (TJXY_app's Electron preload and the
 * Android WebView bridge). Shells that can install updates define `window.tjxyUpdater` before the
 * app starts; browsers leave it undefined, so the update UI stays hidden there.
 */
export interface NativeAppInfo {
  platform: 'windows' | 'macos' | 'linux' | 'android' | 'ios';
  arch: string;
  version: string;
}

export interface NativeInstallRequest {
  url: string;
  headers: Record<string, string>;
  fileName: string;
  sha256: string;
  size: number;
}

export interface NativeInstallResult {
  outcome: 'restarting' | 'installer-opened';
}

export interface NativeUpdater {
  getAppInfo(): Promise<NativeAppInfo>;
  install(request: NativeInstallRequest): Promise<NativeInstallResult>;
  onProgress?(listener: (received: number, total: number) => void): () => void;
}

declare global {
  interface Window {
    tjxyUpdater?: NativeUpdater;
  }
}

export function getNativeUpdater(): NativeUpdater | null {
  return typeof window === 'undefined' ? null : window.tjxyUpdater ?? null;
}
