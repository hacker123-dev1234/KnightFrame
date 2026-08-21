<script lang="ts">
  import { translate } from '../../i18n';
  import type { Locale } from '../../types';
  import type { MarketPromptState } from '../../market-state';

  export let locale: Locale;
  export let prompts: MarketPromptState;
  export let strategyFiles: string[];
  export let usage: Record<string, unknown> | undefined;

  $: t = (key: string) => translate(locale, key);

  $: view = [
    { key: 'market.debug.system', text: prompts.stage1System },
    { key: 'market.debug.user', text: prompts.stage1User },
    { key: 'market.debug.system', text: prompts.stage2System },
    { key: 'market.debug.user', text: prompts.stage2User },
  ];
  $: filled = view.filter((section) => section.text);
  $: usagePairs = usage ? Object.entries(usage) : [];
</script>

<div class="debug-panel">
  {#if !filled.length && !strategyFiles.length}
    <div class="debug-empty"><p>{t('market.debug.empty')}</p></div>
  {:else}
    <div class="debug-scroll">
      {#if strategyFiles.length}
        <section class="debug-block">
          <h4>{t('market.debug.files')}</h4>
          <ul class="file-list">
            {#each strategyFiles as file (file)}
              <li>{file}</li>
            {/each}
          </ul>
        </section>
      {/if}
      {#if usagePairs.length}
        <section class="debug-block">
          <h4>{t('market.usage')}</h4>
          <div class="usage-grid">
            {#each usagePairs as [key, value] (key)}
              <span class="usage-key">{key}</span>
              <span class="usage-value">{String(value)}</span>
            {/each}
          </div>
        </section>
      {/if}
      {#each filled as section, index (index)}
        <section class="debug-block">
          <h4>{t(section.key)} · {index + 1}</h4>
          <pre>{section.text}</pre>
        </section>
      {/each}
    </div>
  {/if}
</div>

<style>
  .debug-panel {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    min-height: 0;
  }
  .debug-empty {
    display: grid;
    place-items: center;
    height: 100%;
    color: var(--dim);
    font-size: 12px;
  }
  .debug-empty p { margin: 0; }
  .debug-scroll {
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: #2d2d2d transparent;
  }
  .debug-block {
    margin-bottom: 12px;
    padding: 11px 13px;
    border: 1px solid var(--border-soft);
    border-radius: var(--radius-md);
    background: var(--surface-1);
  }
  .debug-block h4 {
    margin: 0 0 8px;
    color: var(--dim);
    font: 600 9px var(--mono);
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }
  .file-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    color: var(--text-soft);
    font: 11px var(--mono);
    list-style: none;
  }
  .file-list li::before { content: '› '; color: var(--dim); }
  .usage-grid {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 4px 14px;
  }
  .usage-key { color: var(--dim); font: 10px var(--mono); }
  .usage-value { color: var(--text-soft); font: 10px var(--mono); text-align: right; }
  .debug-block pre {
    max-height: 260px;
    margin: 0;
    overflow: auto;
    color: var(--muted);
    font: 10px/1.6 var(--mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    scrollbar-width: thin;
    scrollbar-color: #2d2d2d transparent;
  }
</style>
