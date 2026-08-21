<script lang="ts">
  export let name:
    | 'plus'
    | 'panel'
    | 'settings'
    | 'browser'
    | 'tasks'
    | 'usage'
    | 'send'
    | 'stop'
    | 'chevron'
    | 'back'
    | 'forward'
    | 'refresh'
    | 'minimize'
    | 'maximize'
    | 'close'
    | 'spark'
    | 'shield'
    | 'check'
    | 'alert'
    | 'clock'
    | 'copy'
    | 'paperclip'
    | 'more'
    | 'edit'
    | 'trash'
    | 'graph'
    | 'market'
    | 'desktop'
    | 'cloud'
    | 'play'
    | 'pause' = 'spark';
  export let size = 18;
  export let strokeWidth = 2;
</script>

<svg
  class="kf-icon"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width={strokeWidth}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#if name === 'plus'}<path d="M12 5v14M5 12h14" />
  {:else if name === 'panel'}<rect x="3" y="4" width="18" height="16" rx="3" /><path d="M9 4v16" />
  {:else if name === 'settings'}<circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .34 1.88l.06.06-2.83 2.83-.06-.06a1.7 1.7 0 0 0-1.88-.34 1.7 1.7 0 0 0-1.03 1.55V21h-4v-.08A1.7 1.7 0 0 0 9 19.37a1.7 1.7 0 0 0-1.88.34l-.06.06-2.83-2.83.06-.06A1.7 1.7 0 0 0 4.63 15 1.7 1.7 0 0 0 3.08 14H3v-4h.08A1.7 1.7 0 0 0 4.63 9a1.7 1.7 0 0 0-.34-1.88l-.06-.06 2.83-2.83.06.06A1.7 1.7 0 0 0 9 4.63 1.7 1.7 0 0 0 10 3.08V3h4v.08A1.7 1.7 0 0 0 15 4.63a1.7 1.7 0 0 0 1.88-.34l.06-.06 2.83 2.83-.06.06A1.7 1.7 0 0 0 19.37 9 1.7 1.7 0 0 0 20.92 10H21v4h-.08A1.7 1.7 0 0 0 19.4 15Z" />
  {:else if name === 'browser'}<circle cx="12" cy="12" r="9" /><path d="M3 12h18M12 3c2.3 2.4 3.5 5.4 3.5 9s-1.2 6.6-3.5 9c-2.3-2.4-3.5-5.4-3.5-9S9.7 5.4 12 3Z" />
  {:else if name === 'tasks'}<path d="M8 6h13M8 12h13M8 18h13" /><path d="m3 6 1 1 2-2m-3 7 1 1 2-2m-3 7 1 1 2-2" />
  {:else if name === 'usage'}<path d="M4 19V9m5 10V5m5 14v-7m5 7V3" />
  {:else if name === 'send'}<path d="m5 12 14-7-5 14-2.5-5.5L5 12Z" /><path d="M11.5 13.5 19 5" />
  {:else if name === 'stop'}<rect x="7" y="7" width="10" height="10" rx="2" />
  {:else if name === 'chevron'}<path d="m9 18 6-6-6-6" />
  {:else if name === 'back'}<path d="m15 18-6-6 6-6" />
  {:else if name === 'forward'}<path d="m9 18 6-6-6-6" />
  {:else if name === 'refresh'}<path d="M20 11a8 8 0 1 0-2.34 5.66M20 4v7h-7" />
  {:else if name === 'minimize'}<path d="M5 12h14" />
  {:else if name === 'maximize'}<rect x="5" y="5" width="14" height="14" rx="1.5" />
  {:else if name === 'close'}<path d="M6 6l12 12M18 6 6 18" />
  {:else if name === 'shield'}<path d="M12 3 4.5 6v5.5c0 4.6 3.1 7.8 7.5 9.5 4.4-1.7 7.5-4.9 7.5-9.5V6L12 3Z" />
  {:else if name === 'check'}<path d="m5 12 4 4L19 6" />
  {:else if name === 'alert'}<path d="M12 4 3.5 19h17L12 4Z" /><path d="M12 9v4m0 3v.01" />
  {:else if name === 'clock'}<circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 2" />
  {:else if name === 'copy'}<rect x="8" y="8" width="11" height="11" rx="2" /><path d="M16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h3" />
  {:else if name === 'paperclip'}<path d="m20.5 11.5-8.8 8.8a5 5 0 0 1-7.1-7.1l9.2-9.2a3.5 3.5 0 1 1 5 5l-9.2 9.2a2 2 0 1 1-2.8-2.8l8.5-8.5" />
  {:else if name === 'more'}<circle cx="5" cy="12" r="1" fill="currentColor" stroke="none" /><circle cx="12" cy="12" r="1" fill="currentColor" stroke="none" /><circle cx="19" cy="12" r="1" fill="currentColor" stroke="none" />
  {:else if name === 'edit'}<path d="m4 20 4.2-1 10.5-10.5a2.1 2.1 0 0 0-3-3L5.2 16 4 20Z" /><path d="m14.5 6.5 3 3" />
  {:else if name === 'trash'}<path d="M4 7h16M9 7V4h6v3m3 0-1 13H7L6 7m4 4v5m4-5v5" />
  {:else if name === 'graph'}<circle cx="5" cy="6" r="2" /><circle cx="18" cy="5" r="2" /><circle cx="8" cy="18" r="2" /><circle cx="19" cy="17" r="2" /><path d="m7 6 9-1M6 8l2 8m2-1 6-8m-6 11 7-1" />
  {:else if name === 'market'}<path d="M4 19V5m0 14h16" /><path d="m7.5 15-3-3.5M11 15V9.5M14.5 15v-8M18 15v-4" />
  {:else if name === 'desktop'}<rect x="3" y="4" width="18" height="13" rx="2.5" /><path d="M8 21h8m-4-4v4" />
  {:else if name === 'cloud'}<path d="M7 18h10a4 4 0 0 0 .8-7.92A6 6 0 0 0 6.2 9.1 4.5 4.5 0 0 0 7 18Z" />
  {:else if name === 'play'}<path d="M7 4.8v14.4L19 12 7 4.8Z" />
  {:else if name === 'pause'}<path d="M8 5v14M16 5v14" />
  {:else}<path d="m12 3 1.45 4.55L18 9l-4.55 1.45L12 15l-1.45-4.55L6 9l4.55-1.45L12 3Z" /><path d="m18.5 15 .75 2.25L21.5 18l-2.25.75L18.5 21l-.75-2.25L15.5 18l2.25-.75.75-2.25Z" />{/if}
</svg>
