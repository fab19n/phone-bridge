/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** "true" in `--mode spikes` builds (app/.env.spikes). */
  readonly VITE_SPIKES?: string;
}
