<script lang="ts">
  import Icon from '../Icon.svelte';
  import { translate } from '../../i18n';
  import type { Locale, MarketRecordSummary } from '../../types';

  export let locale: Locale;
  export let records: MarketRecordSummary[];
  export let loading: boolean;
  export let onLoad: (file: string) => void;
  export let onRefresh: () => void;

  $: t = (key: string) => translate(locale, key);

  function metaString(record: MarketRecordSummary, key: string): string {
    const value = record.meta?.[key];
    return typeof value === 'string' ? value : typeof value === 'number' ? String(value) : '';
  }

  function timeOf(record: MarketRecordSummary): string {
    const raw = metaString(record, 'ts_local');
    const ts = Number(raw) || metaString(record, 'ts_ms');
    if (!ts) return '';
    const date = new Date(Number(ts) * (Number(ts) < 1e12 ? 1000 : 1));
    return `${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')} ${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`;
  }
</script>

<div class="records-panel">
  <header class="records-head">
    <h4>{t('market.records.title')}</h4>
    <button type="button" on:click={onRefresh} aria-label={t('market.records.refresh')} title={t('market.records.refresh')}>
      <Icon name="refresh" size={14} />
    </button>
  </header>

  {#if !records.length && !loading}
    <div class="records-empty"><p>{t('market.records.empty')}</p></div>
  {:else}
    <div class="records-scroll">
      {#each records as record (record.file)}
        <button type="button" class="record-row" on:click={() => onLoad(record.file)}>
          <span class="record-main">
            <strong>{metaString(record, 'symbol') || '—'} · {metaString(record, 'timeframe') || '—'}</strong>
            <small>{timeOf(record)}</small>
          </span>
          <span class="record-tags">
            {#if record.hasDecision}<em class="tag decision">{t('market.records.decision')}</em>{/if}
            {#if record.partial}<em class="tag partial">{t('market.records.partial')}</em>{/if}
          </span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .records-panel {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 10px;
    width: 100%;
    height: 100%;
    min-height: 0;
  }
  .records-head { display: flex; align-items: center; justify-content: space-between; }
  .records-head h4 {
    margin: 0;
    color: var(--dim);
    font: 600 9px var(--mono);
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }
  .records-head button {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 9px;
    color: var(--muted);
    background: transparent;
    cursor: pointer;
    transition: color 0.4s var(--ease-soft), background-color 0.4s var(--ease-soft);
  }
  .records-head button:hover { color: var(--text); background: rgba(255, 255, 255, 0.055); }
  .records-empty {
    display: grid;
    place-items: center;
    height: 100%;
    color: var(--dim);
    font-size: 12px;
  }
  .records-empty p { margin: 0; }
  .records-scroll {
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: #2d2d2d transparent;
  }
  .record-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    width: 100%;
    margin-bottom: 6px;
    padding: 9px 11px;
    border: 1px solid var(--border-soft);
    border-radius: 11px;
    color: var(--text-soft);
    background: var(--surface-1);
    cursor: pointer;
    text-align: left;
    transition: border-color 0.4s var(--ease-soft), background-color 0.4s var(--ease-soft), transform 0.4s var(--ease-out);
  }
  .record-row:hover { border-color: var(--border); background: #131313; transform: translateX(2px); }
  .record-main { display: grid; gap: 3px; min-width: 0; }
  .record-main strong { overflow: hidden; font: 600 12px var(--serif); text-overflow: ellipsis; white-space: nowrap; }
  .record-main small { color: var(--dim); font: 10px var(--mono); }
  .record-tags { display: flex; flex: 0 0 auto; gap: 5px; }
  .tag {
    padding: 2px 7px;
    border: 1px solid var(--border-soft);
    border-radius: 999px;
    color: var(--muted);
    font: 500 9px var(--serif);
    font-style: normal;
  }
  .tag.decision { color: #b9d0be; border-color: #31402f; }
  .tag.partial { color: #c9b58e; border-color: #453a26; }
</style>
