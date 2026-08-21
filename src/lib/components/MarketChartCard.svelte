<script lang="ts">
  // 会话内市场工具的直出图表卡片：完整 K 线 + EMA20 曲线 + 讲解指标。
  // 不进工作流折叠 —— 行情是给人看的，直接展开。
  import Icon from './Icon.svelte';
  import { marketToolCharts } from '../market-state';
  import { setPage } from '../state';
  import { bridge } from '../bridge';
  import { translate } from '../i18n';
  import type { KlineBar, Locale, ReceiptStatus } from '../types';

  export let locale: Locale;
  export let callId: string;
  export let status: ReceiptStatus;

  $: t = (key: string, args: Record<string, string | number> = {}) => translate(locale, key, args);
  $: chart = $marketToolCharts[callId];
  $: frame = chart?.frame;
  // bars 新→旧；绘图需要旧→新
  let bars: KlineBar[] = [];
  let emaSeries: (number | null)[] = [];
  $: bars = frame ? [...frame.bars].reverse() : [];
  $: emaSeries = frame ? [...frame.indicators.ema20].reverse() : [];
  $: lastBar = bars.length ? bars[bars.length - 1] : undefined;
  $: firstBar = bars.length ? bars[0] : undefined;
  $: windowChangePct = lastBar && firstBar && firstBar.close > 0
    ? (lastBar.close / firstBar.close - 1) * 100
    : undefined;
  $: latestEma = [...emaSeries].reverse().find((value): value is number => value !== null);
  $: closeVsEmaPct = lastBar && latestEma && latestEma > 0
    ? (lastBar.close / latestEma - 1) * 100
    : undefined;
  $: latestAtr = frame ? [...frame.indicators.atr14].reverse().find((value): value is number => value !== null) : undefined;
  $: atrPct = lastBar && latestAtr && lastBar.close > 0 ? (latestAtr / lastBar.close) * 100 : undefined;
  $: up = (windowChangePct ?? 0) >= 0;

  const VIEW_W = 780;
  const VIEW_H = 300;
  const PAD = { left: 8, right: 62, top: 16, bottom: 40 };
  const VOL_H = 44;
  $: plotW = VIEW_W - PAD.left - PAD.right;
  $: plotH = VIEW_H - PAD.top - PAD.bottom - VOL_H - 6;
  $: visible = bars.slice(-160);
  $: visibleEma = emaSeries.slice(-160);
  $: priceLow = visible.length ? Math.min(...visible.map((bar) => bar.low)) * 0.998 : 0;
  $: priceHigh = visible.length ? Math.max(...visible.map((bar) => bar.high)) * 1.002 : 1;
  $: maxVolume = visible.length ? Math.max(...visible.map((bar) => bar.volume), 1) : 1;
  $: step = visible.length ? plotW / visible.length : plotW;
  $: candleW = Math.max(1.5, Math.min(11, step * 0.62));

  function x(index: number): number {
    return PAD.left + step * (index + 0.5);
  }
  function y(price: number): number {
    const ratio = (price - priceLow) / (priceHigh - priceLow || 1);
    return PAD.top + plotH - ratio * plotH;
  }
  function volY(volume: number): number {
    const bandTop = PAD.top + plotH + 6;
    const bandH = VOL_H - 8;
    return bandTop + bandH * (1 - volume / maxVolume);
  }
  function priceLabel(price: number): string {
    const digits = price >= 1000 ? 0 : price >= 10 ? 2 : 4;
    return price.toFixed(digits);
  }
  function timeLabel(bar: KlineBar, index: number, total: number): string | undefined {
    if (total < 4) return undefined;
    const ticks = total > 80 ? 5 : 3;
    if (index % Math.ceil(total / ticks) !== 0 && index !== total - 1) return undefined;
    const date = new Date(bar.tsOpen);
    const daily = frame?.timeframe === '1d' || frame?.timeframe === '1w';
    return new Intl.DateTimeFormat(locale, daily
      ? { month: 'short', day: 'numeric' }
      : { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }).format(date);
  }
  $: emaPath = visibleEma
    .map((value, index) => (value === null ? null : `${x(index).toFixed(1)},${y(value).toFixed(1)}`))
    .filter((point): point is string => point !== null)
    .join(' ');
  $: gridLines = [0.2, 0.4, 0.6, 0.8].map((ratio) => ({
    y: PAD.top + plotH - ratio * plotH,
    price: priceLow + ratio * (priceHigh - priceLow),
  }));

  async function openInMarket() {
    if (!frame) return;
    setPage('market');
    try {
      await bridge.marketSubscribe({
        source: chart?.source || 'eastmoney',
        symbol: frame.symbol,
        timeframe: frame.timeframe,
      });
    } catch {
      // 订阅失败不打断导航；市场页可手动重试
    }
  }

  $: trendKey = closeVsEmaPct === undefined ? 'market.card.trend.flat'
    : closeVsEmaPct > 0.35 ? 'market.card.trend.up'
    : closeVsEmaPct < -0.35 ? 'market.card.trend.down'
    : 'market.card.trend.flat';
  $: volKey = atrPct === undefined ? 'market.card.vol.mid'
    : atrPct > 1.4 ? 'market.card.vol.high'
    : atrPct > 0.5 ? 'market.card.vol.mid'
    : 'market.card.vol.low';
</script>

<section class="market-card" aria-label={t('market.card.title', { symbol: frame?.symbol ?? '' })}>
  {#if !frame}
    <div class="market-card-pending" class:failed={status === 'failed'}>
      {#if status === 'failed'}
        <Icon name="alert" size={18} />
        <span>{t('market.card.failed')}</span>
      {:else}
        <span class="pending-pulse"></span>
        <span>{t('market.card.loading')}</span>
      {/if}
    </div>
  {:else}
    <header class="market-card-head">
      <div class="market-card-title">
        <strong>{frame.symbol}</strong>
        <span class="chip">{frame.timeframe}</span>
        <span class="chip dim">{chart?.source}</span>
        <span class="chip dim">{t('market.card.bars', { count: bars.length })}</span>
      </div>
      <div class="market-card-price">
        <strong class:up class:down={!up}>{lastBar ? priceLabel(lastBar.close) : '—'}</strong>
        {#if windowChangePct !== undefined}
          <span class="delta" class:up class:down={!up}>
            {up ? '▲' : '▼'} {Math.abs(windowChangePct).toFixed(2)}%
          </span>
        {/if}
        <button type="button" on:click={openInMarket} title={t('market.card.enter')}>
          {t('market.card.enter')}<Icon name="graph" size={13} />
        </button>
      </div>
    </header>

    <div class="market-chart-wrap">
      <svg viewBox="0 0 {VIEW_W} {VIEW_H}" role="img" aria-label={t('market.card.chartLabel', { symbol: frame.symbol })} preserveAspectRatio="none">
        <defs>
          <linearGradient id="mkc-glow" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stop-color="rgba(255,255,255,0.05)" />
            <stop offset="100%" stop-color="rgba(255,255,255,0)" />
          </linearGradient>
        </defs>
        <rect x={PAD.left} y={PAD.top} width={plotW} height={plotH} fill="url(#mkc-glow)" />
        {#each gridLines as line (line.y)}
          <line x1={PAD.left} x2={PAD.left + plotW} y1={line.y} y2={line.y} class="grid" />
          <text x={PAD.left + plotW + 6} y={line.y + 3} class="axis">{priceLabel(line.price)}</text>
        {/each}
        {#each visible as bar, index (bar.seq)}
          <line x1={x(index)} x2={x(index)} y1={y(bar.high)} y2={y(bar.low)} class="wick" class:bar-up={bar.close >= bar.open} class:bar-down={bar.close < bar.open} />
          <rect
            x={x(index) - candleW / 2}
            y={y(Math.max(bar.open, bar.close))}
            width={candleW}
            height={Math.max(1, Math.abs(y(bar.open) - y(bar.close)))}
            class="candle"
            class:bar-up={bar.close >= bar.open}
            class:bar-down={bar.close < bar.open}
          />
          <rect x={x(index) - candleW / 2} y={volY(bar.volume)} width={candleW} height={(PAD.top + plotH + 6 + VOL_H - 8) - volY(bar.volume)} class="volume" class:bar-up={bar.close >= bar.open} class:bar-down={bar.close < bar.open} />
          {#if timeLabel(bar, index, visible.length)}
            <text x={x(index)} y={VIEW_H - 12} class="axis time" text-anchor="middle">{timeLabel(bar, index, visible.length)}</text>
          {/if}
        {/each}
        {#if emaPath}
          <polyline points={emaPath} class="ema" />
        {/if}
        {#if lastBar}
          <line x1={PAD.left} x2={PAD.left + plotW} y1={y(lastBar.close)} y2={y(lastBar.close)} class="last-price" />
          <rect x={PAD.left + plotW + 2} y={y(lastBar.close) - 8} width="56" height="16" rx="4" class="last-chip" />
          <text x={PAD.left + plotW + 30} y={y(lastBar.close) + 4} class="axis chip-text" text-anchor="middle">{priceLabel(lastBar.close)}</text>
        {/if}
      </svg>
    </div>

    <div class="market-card-explain">
      <div class="explain-tile">
        <span class="tile-label">{t('market.card.trend')}</span>
        <strong>{t(trendKey)}{closeVsEmaPct !== undefined ? ` · ${closeVsEmaPct > 0 ? '+' : ''}${closeVsEmaPct.toFixed(2)}%` : ''}</strong>
        <small>{t('market.card.explain.trend')}</small>
      </div>
      <div class="explain-tile">
        <span class="tile-label">{t('market.card.volatility')}</span>
        <strong>{t(volKey)}{atrPct !== undefined ? ` · ${atrPct.toFixed(2)}%` : ''}</strong>
        <small>{t('market.card.explain.volatility')}</small>
      </div>
      <div class="explain-tile">
        <span class="tile-label">{t('market.card.window')}</span>
        <strong class:up class:down={!up}>{windowChangePct !== undefined ? `${windowChangePct > 0 ? '+' : ''}${windowChangePct.toFixed(2)}%` : '—'}</strong>
        <small>{t('market.card.explain.window')}</small>
      </div>
    </div>
  {/if}
</section>

<style>
  .market-card {
    display: grid;
    gap: 10px;
    margin: 6px 0 10px;
    padding: 14px 16px 12px;
    border: 1px solid var(--border, #2a2a2a);
    border-radius: 16px;
    background:
      radial-gradient(120% 90% at 18% 0%, rgba(255, 255, 255, 0.035), transparent 52%),
      var(--surface-1, #0c0c0c);
    box-shadow: 0 18px 48px rgba(0, 0, 0, 0.4), 0 0 26px rgba(255, 255, 255, 0.02);
    animation: cardIn 0.5s var(--ease-out, ease-out) both;
  }
  @keyframes cardIn {
    from { opacity: 0; transform: translateY(6px) scale(0.992); }
    to { opacity: 1; transform: none; }
  }
  .market-card-pending {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 92px;
    color: var(--muted, #9a9a9a);
    font: 500 12px var(--serif, serif);
  }
  .market-card-pending.failed { color: #c9a07a; }
  .pending-pulse {
    width: 22px;
    height: 2px;
    border-radius: 2px;
    background: rgba(255, 255, 255, 0.25);
    position: relative;
    overflow: hidden;
  }
  .pending-pulse::after {
    content: '';
    position: absolute;
    inset: 0;
    width: 10px;
    background: #fff;
    box-shadow: 0 0 8px rgba(255, 255, 255, 0.7);
    animation: sweep 1.1s ease-in-out infinite;
  }
  @keyframes sweep {
    from { transform: translateX(-12px); }
    to { transform: translateX(24px); }
  }
  .market-card-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }
  .market-card-title { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .market-card-title strong {
    color: var(--text, #eee);
    font: 500 19px/1.1 var(--serif, serif);
    letter-spacing: 0.01em;
  }
  .chip {
    padding: 2.5px 8px;
    border: 1px solid var(--border, #333);
    border-radius: 999px;
    color: var(--text-soft, #ccc);
    font: 500 9.5px var(--mono, monospace);
  }
  .chip.dim { color: var(--dim, #777); border-color: var(--border-soft, #262626); }
  .market-card-price { display: flex; align-items: center; gap: 10px; }
  .market-card-price strong { font: 500 21px/1 var(--serif, serif); letter-spacing: 0.01em; }
  .delta { font: 600 11px var(--mono, monospace); }
  .market-card-price strong.up, .delta.up { color: #f2f2f2; text-shadow: 0 0 14px rgba(255, 255, 255, 0.25); }
  .market-card-price strong.down, .delta.down { color: #9b9b9b; }
  .market-card-price button {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 12px;
    border: 1px solid var(--border, #3a3a3a);
    border-radius: 10px;
    color: var(--text-soft, #ddd);
    background: #131313;
    cursor: pointer;
    font: 600 10.5px var(--serif, serif);
    transition: border-color 0.35s var(--ease-soft, ease), box-shadow 0.5s var(--ease-soft, ease), transform 0.3s var(--ease-out, ease-out), background-color 0.35s ease;
  }
  .market-card-price button:hover {
    border-color: #6a6a6a;
    background: #1a1a1a;
    box-shadow: 0 0 20px rgba(255, 255, 255, 0.07);
    transform: translateY(-1px);
  }
  .market-chart-wrap { margin: 0 -4px; }
  svg { display: block; width: 100%; height: auto; }
  .grid { stroke: rgba(255, 255, 255, 0.05); stroke-width: 1; }
  .axis { fill: #6f6f6f; font: 8.5px var(--mono, monospace); }
  .axis.time { fill: #5d5d5d; }
  .wick { stroke-width: 1; }
  .candle { stroke-width: 0.8; }
  .bar-up { fill: #e6e6e6; stroke: #f0f0f0; }
  line.bar-up, .wick.bar-up { stroke: #dcdcdc; }
  rect.bar-up.volume { fill: rgba(230, 230, 230, 0.28); stroke: none; }
  .bar-down { fill: #101010; stroke: #787878; }
  line.bar-down, .wick.bar-down { stroke: #6e6e6e; }
  rect.bar-down.volume { fill: rgba(120, 120, 120, 0.22); stroke: none; }
  .volume { stroke: none; }
  .ema {
    fill: none;
    stroke: rgba(255, 255, 255, 0.6);
    stroke-width: 1.4;
    stroke-linejoin: round;
    stroke-linecap: round;
    filter: drop-shadow(0 0 5px rgba(255, 255, 255, 0.28));
  }
  .last-price { stroke: rgba(255, 255, 255, 0.3); stroke-width: 0.8; stroke-dasharray: 3 4; }
  .last-chip { fill: #e9e9e9; }
  .chip-text { fill: #101010; font-weight: 600; }
  .market-card-explain {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
  }
  .explain-tile {
    display: grid;
    gap: 3px;
    padding: 9px 11px;
    border: 1px solid var(--border-soft, #232323);
    border-radius: 11px;
    background: rgba(255, 255, 255, 0.012);
  }
  .tile-label {
    color: var(--dim, #6f6f6f);
    font: 600 8.5px var(--serif, serif);
    letter-spacing: 0.09em;
    text-transform: uppercase;
  }
  .explain-tile strong { color: var(--text-soft, #ddd); font: 500 12.5px var(--serif, serif); }
  .explain-tile strong.up { color: #f0f0f0; }
  .explain-tile strong.down { color: #9b9b9b; }
  .explain-tile small { color: var(--dim, #666); font: 9.5px/1.5 var(--serif, serif); }
  @media (max-width: 720px) {
    .market-card-explain { grid-template-columns: 1fr; }
  }
</style>
