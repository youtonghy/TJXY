/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_TJXY_SHELL?: string;
  readonly VITE_TJXY_VERSION?: string;
  readonly VITE_TJXY_COMMIT?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
