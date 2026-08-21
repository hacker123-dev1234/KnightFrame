<script lang="ts">
  import { translate } from '../../i18n';
  import type { AnalysisRecord, Locale } from '../../types';

  export let locale: Locale;
  export let record: AnalysisRecord | undefined;

  type Tab = 'stage1' | 'stage2';
  let tab: Tab = 'stage1';

  $: t = (key: string) => translate(locale, key);
  $: payload = tab === 'stage1' ? record?.stage1Diagnosis : record?.stage2Decision;
  // 大 JSON 全量 stringify + 渲染会冻结 webview；截断到上限并给出提示。
  const RAW_CHAR_LIMIT = 24000;
  $: rawFull = payload ? JSON.stringify(payload, null, 2) : '';
  $: text = rawFull.length > RAW_CHAR_LIMIT
    ? `${rawFull.slice(0, RAW_CHAR_LIMIT)}\n… (${t('market.raw.truncated')})`
    : rawFull;
</script>

<div class="raw-panel">
  {#if !text}
    <div class="raw-empty"><p>{t('market.raw.empty')}</p></div>
  {:else}
    <div class="raw-tabs" role="tablist">
      <button type="button" role="tab" class:active={tab === 'stage1'} on:click={() => (tab = 'stage1')}>
        {t('market.raw.stage1')}
      </button>
      <button
        type="button"
        role="tab"
        class:active={tab === 'stage2'}
        disabled={!record?.stage2Decision}
        on:click={() => (tab = 'stage2')}
      >
        {t('market.raw.stage2')}
      </button>
    </div>
    <pre class="raw-json">{text}</pre>
  {/if}
</div>

<style>
  .raw-panel {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 10px;
    width: 100%;
    height: 100%;
    min-height: 0;
  }
  .raw-empty {
    display: grid;
    place-items: center;
    height: 100%;
    color: var(--dim);
    font-size: 12px;
  }
  .raw-empty p { margin: 0; }
  .raw-tabs {
    display: flex;
    padding: 3px;
    border: 1px solid var(--border-soft);
    border-radius: 11px;
    background: #0b0b0b;
  }
  .raw-tabs button {
    height: 30px;
    padding: 0 12px;
    border: 0;
    border-radius: 8px;
    color: var(--muted);
    background: transparent;
    cursor: pointer;
    font: 500 11px var(--serif);
    transition: color 0.4s var(--ease-soft), background-color 0.4s var(--ease-soft);
  }
  .raw-tabs button:hover:not(:disabled) { color: var(--text-soft); }
  .raw-tabs button.active {
    color: var(--inverse);
    background: #e9e9e9;
  }
  .raw-json {
    min-height: 0;
    margin: 0;
    overflow: auto;
    padding: 12px;
    border: 1px solid var(--border-soft);
    border-radius: var(--radius-md);
    color: var(--text-soft);
    background: var(--surface-1);
    font: 11px/1.62 var(--mono);
    white-space: pre;
    scrollbar-width: thin;
    scrollbar-color: #2d2d2d transparent;
  }
</style>
