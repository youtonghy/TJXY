/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_TJXY_SHELL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
