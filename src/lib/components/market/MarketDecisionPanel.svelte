<script lang="ts">
  import Icon from '../Icon.svelte';
  import { translate } from '../../i18n';
  import type { AnalysisRecord, Locale } from '../../types';

  export let locale: Locale;
  export let record: AnalysisRecord | undefined;

  $: t = (key: string) => translate(locale, key);
  $: decision = (record?.stage2Decision as Record<string, unknown> | undefined)?.decision as
    | Record<string, unknown>
    | undefined;
  $: summary = (record?.stage2Decision as Record<string, unknown> | undefined)?.diagnosis_summary as
    | Record<string, unknown>
    | undefined;
  $: terminal = (record?.stage2Decision as Record<string, unknown> | undefined)?.terminal as
    | Record<string, unknown>
    | undefined;
  $: orderType = typeof decision?.order_type === 'string' ? decision.order_type : '';
  $: noOrder = orderType === '不下单';
  let prices: [string, unknown][] = [];
  $: prices = decision
    ? Object.entries(decision).filter(([key]) =>
        ['entry_price', 'take_profit_price', 'stop_loss_price'].includes(key))
    : [];
  let longTexts: [string, unknown][] = [];
  $: longTexts = decision
    ? Object.entries(decision).filter(([key, value]) =>
        typeof value === 'string' && value.length > 40
        && ['reasoning', 'key_factors', 'watch_points', 'risk_assessment', 'invalidation_condition', 'trade_confidence_reasoning', 'estimated_win_rate_reasoning'].includes(key))
    : [];
</script>

<div class="decision-panel">
  {#if !decision}
    <div class="decision-empty">
      <Icon name="market" size={26} />
      <p>{t('market.decision.empty')}</p>
    </div>
  {:else}
    <div class="decision-scroll">
      <div class="decision-headline" class:no-order={noOrder}>
        <div class="headline-main">
          <strong class="order-type">{orderType}</strong>
          {#if !noOrder && typeof decision.order_direction === 'string'}
            <span class="order-direction long={String(decision.order_direction).includes('多')}">
              {decision.order_direction}
            </span>
          {/if}
        </div>
        {#if terminal?.outcome}
          <span class="terminal-outcome outcome-{terminal.outcome}">
            {String(terminal.node_id ?? '')} · {String(terminal.outcome)}
          </span>
        {/if}
      </div>

      {#if summary}
        <div class="summary-strip">
          {#if summary.cycle_position}<span>{String(summary.cycle_position)}</span>{/if}
          {#if summary.direction}<span class:dir-bull={summary.direction === 'bullish'} class:dir-bear={summary.direction === 'bearish'}>{String(summary.direction)}</span>{/if}
          {#if Array.isArray(summary.key_signals)}
            {#each summary.key_signals.slice(0, 3) as signal (String(signal))}
              <em>{String(signal)}</em>
            {/each}
          {/if}
        </div>
      {/if}

      {#if prices.length}
        <div class="price-grid">
          {#each prices as [key, value] (key)}
            <div class="price-cell">
              <span class="price-key">{key}</span>
              <strong class="price-value">{String(value)}</strong>
            </div>
          {/each}
          {#if typeof decision.trade_confidence === 'number'}
            <div class="price-cell">
              <span class="price-key">trade_confidence</span>
              <strong class="price-value">{decision.trade_confidence}%</strong>
            </div>
          {/if}
          {#if typeof decision.estimated_win_rate === 'number'}
            <div class="price-cell">
              <span class="price-key">estimated_win_rate</span>
              <strong class="price-value">{decision.estimated_win_rate}%</strong>
            </div>
          {/if}
        </div>
      {/if}

      {#each longTexts as [key, value] (key)}
        <section class="decision-block">
          <h4>{key}</h4>
          <p>{String(value)}</p>
        </section>
      {/each}
    </div>
  {/if}
</div>

<style>
  .decision-panel {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    min-height: 0;
  }
  .decision-empty {
    display: grid;
    place-items: center;
    gap: 9px;
    height: 100%;
    color: var(--dim);
    font-size: 12px;
    text-align: center;
  }
  .decision-empty p { margin: 0; }
  .decision-scroll {
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: #2d2d2d transparent;
  }
  .decision-headline {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 10px;
    padding: 12px 14px;
    border: 1px solid var(--border-soft);
    border-radius: var(--radius-md);
    background: linear-gradient(135deg, #141414, #0e0e0e);
  }
  .headline-main { display: flex; align-items: center; gap: 10px; }
  .order-type { color: var(--text); font: 600 15px var(--serif); letter-spacing: 0.02em; }
  .order-direction {
    padding: 2px 9px;
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--text-soft);
    font: 500 11px var(--serif);
  }
  .order-direction.long { border-color: #3d4a3f; color: #b9d0be; }
  .terminal-outcome {
    color: var(--muted);
    font: 500 10px var(--mono);
    letter-spacing: 0.06em;
  }
  .terminal-outcome.outcome-trade { color: #b9d0be; }
  .terminal-outcome.outcome-wait { color: #c9b58e; }
  .decision-headline.no-order { opacity: 0.85; }
  .summary-strip {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 12px;
  }
  .summary-strip span, .summary-strip em {
    padding: 3px 9px;
    border: 1px solid var(--border-soft);
    border-radius: 999px;
    color: var(--muted);
    background: var(--surface-1);
    font: 500 10px var(--serif);
    font-style: normal;
  }
  .summary-strip span.dir-bull { color: #b9d0be; }
  .summary-strip span.dir-bear { color: #cbb3b3; }
  .price-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
    gap: 8px;
    margin-bottom: 14px;
  }
  .price-cell {
    display: grid;
    gap: 5px;
    padding: 10px 12px;
    border: 1px solid var(--border-soft);
    border-radius: var(--radius-sm);
    background: var(--surface-1);
  }
  .price-key {
    overflow: hidden;
    color: var(--dim);
    font: 500 9px var(--mono);
    letter-spacing: 0.06em;
    text-overflow: ellipsis;
    text-transform: uppercase;
  }
  .price-value { color: var(--text); font: 600 14px var(--mono); }
  .decision-block {
    margin-bottom: 12px;
    padding: 11px 13px;
    border: 1px solid var(--border-soft);
    border-radius: var(--radius-sm);
    background: var(--surface-1);
  }
  .decision-block h4 {
    margin: 0 0 6px;
    color: var(--dim);
    font: 600 9px var(--mono);
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }
  .decision-block p {
    margin: 0;
    color: var(--text-soft);
    font: 12px/1.75 var(--serif);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
