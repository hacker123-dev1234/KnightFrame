/// <reference types="svelte" />

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    __KF_BOOTSTRAP__?: {
      view?: 'mini' | 'settings' | 'browser';
      locale?: 'en-US' | 'zh-CN';
    };
  }
}

export {};
