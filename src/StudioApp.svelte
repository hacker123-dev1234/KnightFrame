<script lang="ts">
  import { onDestroy, onMount, tick } from 'svelte';
  import StudioPage from './lib/pages/StudioPage.svelte';
  import { bridge } from './lib/bridge';
  import type { Locale } from './lib/types';

  let locale: Locale = navigator.language.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en-US';

  onMount(() => {
    let active = true;
    void (async () => {
      try {
        const context = await bridge.pluginStudioBootstrap();
        if (active) locale = context.locale;
      } catch {
        // The platform locale is a complete offline fallback.
      }
      await tick();
      if (active) await bridge.pluginStudioReady().catch(() => undefined);
    })();
    return () => { active = false; };
  });

  function closeWindow() {
    void bridge.stopDshPreview();
    void import('@tauri-apps/api/window').then(({ getCurrentWindow }) => getCurrentWindow().close());
  }

  onDestroy(() => { void bridge.stopDshPreview(); });
</script>

<StudioPage
  {locale}
  onClose={closeWindow}
  onAsk={(request) => bridge.askFromPluginStudio(request)}
  onExport={(document) => bridge.exportPluginStudio(document)}
  onPreview={(document) => bridge.pluginStudioPreview(document)}
  onDshStart={() => bridge.startDshPreview()}
  onDshStop={() => bridge.stopDshPreview()}
/>
