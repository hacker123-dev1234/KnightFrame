<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from '../components/Icon.svelte';
  import Select from '../components/Select.svelte';
  import { translate } from '../i18n';
  import { market, initMarket, fetchMarketData, toggleMarketLive, startMarketAnalysis, stopMarketAnalysis, sendMarketChat, stopMarketChat, loadMarketRecords, loadMarketRecord, updateMarketSettings } from '../market-state';
  import MarketCandleChart from '../components/market/MarketCandleChart.svelte';
  import MarketStreamPanel from '../components/market/MarketStreamPanel.svelte';
  import MarketTreePanel from '../components/market/MarketTreePanel.svelte';
  import MarketFlowPanel from '../components/market/MarketFlowPanel.svelte';
  import MarketDecisionPanel from '../components/market/MarketDecisionPanel.svelte';
  import MarketFuturePanel from '../components/market/MarketFuturePanel.svelte';
  import MarketRawPanel from '../components/market/MarketRawPanel.svelte';
  import MarketDebugPanel from '../components/market/MarketDebugPanel.svelte';
  import MarketSettingsPanel from '../components/market/MarketSettingsPanel.svelte';
  import MarketRecordsPanel from '../components/market/MarketRecordsPanel.svelte';
  import type { Locale } from '../types';

  export let locale: Locale;

  type Tab = 'stream' | 'tree' | 'flow' | 'decision' | 'future' | 'raw' | 'debug';
  type Side = Tab | 'records' | 'settings';

  let tab: Tab = 'stream';
  let side: Side = 'stream';
  let symbol = 'XAUUSD';
  let source = 'tradingview';
  let exchange = 'OANDA';
  let timeframe = '15m';
  let busy = false;

  const sources = ['tradingview', 'yfinance', 'mt5', 'eastmoney'];
  const allTimeframes = ['1m', '5m', '15m', '30m', '1h', '4h', '1d', '1w'];
  const timeframeBySource: Record<string, string[]> = {
    yfinance: ['1m', '5m', '15m', '30m', '1h', '1d', '1w'],
    mt5: ['1m', '5m', '15m', '30m', '1h', '4h', '1d'],
    eastmoney: ['5m', '15m', '30m', '1h', '1d', '1w'],
    tradingview: allTimeframes,
  };
  const tvExchanges = ['OANDA', 'FX', 'BINANCE', 'NASDAQ', 'NYSE', 'COMEX'];

  $: t = (key: string, args: Record<string, string | number> = {}) => translate(locale, key, args);
  $: settings = $market.settings;
  $: frame = $market.frame;
  let tabs: { id: Tab; label: string }[] = [];
  $: tabs = [
    { id: 'stream', label: t('market.tab.stream') },
    { id: 'tree', label: t('market.tab.tree') },
    { id: 'flow', label: t('market.tab.flow') },
    { id: 'decision', label: t('market.tab.decision') },
    { id: 'future', label: t('market.tab.future') },
    { id: 'raw', label: t('market.tab.raw') },
    { id: 'debug', label: t('market.tab.debug') },
  ];

  onMount(() => {
    void initMarket();
  });

  // 设置加载后一次性恢复上次的数据源状态；不能依赖 $market.frame 做
  // 响应式条件——否则任意 market store 更新都会把用户刚选的值弹回。
  let restored = false;
  $: if (!restored && settings) {
    restored = true;
    symbol = settings.general.lastSymbol || symbol;
    source = settings.general.lastDataSource || source;
    exchange = settings.general.lastTradingviewExchange || exchange;
    timeframe = settings.general.lastTimeframe || timeframe;
  }

  function input() {
    return {
      source,
      symbol: symbol.trim().toUpperCase(),
      exchange: source === 'tradingview' ? exchange : undefined,
      timeframe,
    };
  }

  // 数据源切换时收敛周期到该源支持的集合
  $: if (!(timeframeBySource[source] ?? allTimeframes).includes(timeframe)) {
    timeframe = (timeframeBySource[source] ?? allTimeframes)[0];
  }

  async function onFetch() {
    busy = true;
    try {
      await fetchMarketData(input());
    } finally {
      busy = false;
    }
  }

  async function onLive() {
    busy = true;
    try {
      await toggleMarketLive(input());
    } finally {
      busy = false;
    }
  }

  function openSide(next: Side) {
    side = next;
    if (next !== 'records' && next !== 'settings') tab = next;
  }

  async function onChat(text: string) {
    await sendMarketChat(text);
  }
</script>

<section class="market-page">
  <header class="market-toolbar" data-tauri-drag-region>
    <div class="toolbar-group">
      <label class="control">
        <span>{t('market.source')}</span>
        <Select bind:value={source} options={sources.map((item) => ({ value: item, label: t(`market.source.${item}`) }))} />
      </label>
      <label class="control symbol">
        <span>{t('market.symbol')}</span>
        <input bind:value={symbol} placeholder={t('market.symbolPlaceholder')} />
      </label>
      {#if source === 'tradingview'}
        <label class="control">
          <span>{t('market.exchange')}</span>
          <Select bind:value={exchange} options={tvExchanges.map((item) => ({ value: item, label: item }))} />
        </label>
      {/if}
      <label class="control">
        <span>{t('market.timeframe')}</span>
        <Select bind:value={timeframe} options={(timeframeBySource[source] ?? allTimeframes).map((item) => ({ value: item, label: item }))} />
      </label>
    </div>

    <div class="toolbar-actions">
      <button type="button" class="action" disabled={busy} on:click={onFetch}>
        <Icon name="refresh" size={14} />
        <span>{t('market.fetch')}</span>
      </button>
      <button type="button" class="action" class:live={$market.live} disabled={busy} on:click={onLive}>
        <span class="live-dot"></span>
        <span>{t($market.live ? 'market.unsubscribe' : 'market.subscribe')}</span>
      </button>
      {#if $market.analyzing}
        <button type="button" class="action stop" on:click={() => void stopMarketAnalysis()}>
          <Icon name="stop" size={14} />
          <span>{t('market.stopAnalysis')}</span>
        </button>
      {:else}
        <button type="button" class="action primary" disabled={!frame} on:click={() => void startMarketAnalysis(false)}>
          <Icon name="spark" size={14} />
          <span>{t('market.analyze')}</span>
        </button>
      {/if}
    </div>

    <div class="toolbar-meta">
      {#if $market.statusMessage}
        <span class="status-message" class:error={$market.statusError}>{$market.statusMessage}</span>
      {:else if frame}
        <span class="frame-meta">
          {frame.symbol} · {frame.timeframe} · {t('market.barCount', { count: frame.bars.length })}
        </span>
      {:else}
        <span class="frame-meta dim">{t('market.noData')}</span>
      {/if}
      <button
        type="button"
        class="meta-button"
        class:active={side === 'records'}
        aria-label={t('market.records.title')}
        title={t('market.records.title')}
        on:click={() => {
          side = side === 'records' ? 'stream' : 'records';
          if (side === 'records') void loadMarketRecords();
        }}
      >
        <Icon name="clock" size={15} />
      </button>
      <button
        type="button"
        class="meta-button"
        class:active={side === 'settings'}
        aria-label={t('market.settings')}
        title={t('market.settings')}
        on:click={() => (side = side === 'settings' ? 'stream' : 'settings')}
      >
        <Icon name="settings" size={15} />
      </button>
    </div>
  </header>

  <div class="market-body">
    <div class="chart-region">
      {#if frame}
        <MarketCandleChart {frame} />
      {:else}
        <div class="chart-placeholder">
          <Icon name="market" size={30} />
          <p>{t('market.noData')}</p>
        </div>
      {/if}
    </div>

    <aside class="market-side">
      {#if side === 'settings'}
        <MarketSettingsPanel {locale} {settings} onSave={updateMarketSettings} />
      {:else if side === 'records'}
        <MarketRecordsPanel
          {locale}
          records={$market.records}
          loading={$market.recordsLoading}
          onLoad={(file) => void loadMarketRecord(file)}
          onRefresh={() => void loadMarketRecords()}
        />
      {:else}
        <nav class="side-tabs">
          {#each tabs as item (item.id)}
            <button type="button" class:active={tab === item.id} on:click={() => openSide(item.id)}>
              {item.label}
            </button>
          {/each}
        </nav>
        <div class="side-body">
          {#key tab}
            <div class="side-body-inner">
              {#if tab === 'stream'}
                <MarketStreamPanel
                  {locale}
                  stage1={$market.stage1}
                  stage2={$market.stage2}
                  gateWait={$market.gateWait}
                  statusMessage={$market.statusMessage}
                  statusError={$market.statusError}
                  chat={$market.chat}
                  chatDraft={$market.chatDraft}
                  chatStreaming={$market.chatStreaming}
                  canChat={Boolean($market.record)}
                  onSendChat={onChat}
                  onStopChat={() => void stopMarketChat()}
                />
              {:else if tab === 'tree'}
                <MarketTreePanel {locale} record={$market.record} />
              {:else if tab === 'flow'}
                <MarketFlowPanel
                  {locale}
                  record={$market.record}
                  autoPlay={settings?.general.decisionFlowAutoPlay ?? true}
                  playSeconds={settings?.general.decisionFlowPlaySeconds ?? 50}
                />
              {:else if tab === 'decision'}
                <MarketDecisionPanel {locale} record={$market.record} />
              {:else if tab === 'future'}
                <MarketFuturePanel {locale} record={$market.record} />
              {:else if tab === 'raw'}
                <MarketRawPanel {locale} record={$market.record} />
              {:else if tab === 'debug'}
                <MarketDebugPanel
                  {locale}
                  prompts={$market.prompts}
                  strategyFiles={$market.strategyFiles}
                  usage={$market.record?.usageTotal as Record<string, unknown> | undefined}
                />
              {/if}
            </div>
          {/key}
        </div>
      {/if}
    </aside>
  </div>

  <footer class="market-foot">
    <span>{t('market.disclaimer')}</span>
    {#if $market.error}
      <span class="foot-error">{t($market.error.key, $market.error.args ?? {})}</span>
    {/if}
  </footer>
</section>

<style>
  .market-page {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    height: 100%;
    overflow: hidden;
    animation: pageReveal 0.5s var(--ease-out) both;
  }
  .market-toolbar {
    display: flex;
    align-items: flex-end;
    gap: 18px;
    padding: 12px 18px;
    border-bottom: 1px solid var(--border-soft);
  }
  .toolbar-group { display: flex; align-items: flex-end; gap: 9px; }
  .control { display: grid; gap: 4px; min-width: 118px; }
  .control > span {
    color: var(--dim);
    font: 600 9px var(--serif);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .control input {
    height: 32px;
    min-width: 0;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: 10px;
    outline: none;
    color: var(--text);
    background: #0d0d0d;
    font: 12px var(--mono);
    transition: border-color 0.4s var(--ease-soft), box-shadow 0.5s var(--ease-soft);
  }
  .control input:focus {
    border-color: rgba(255, 255, 255, 0.26);
    box-shadow: 0 0 18px rgba(255, 255, 255, 0.05);
  }
  .control.symbol { min-width: 0; }
  .control.symbol input { width: 108px; text-transform: uppercase; }
  .toolbar-actions { display: flex; align-items: flex-end; gap: 7px; }
  .action {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    padding: 0 13px;
    border: 1px solid var(--border);
    border-radius: 10px;
    color: var(--text-soft);
    background: #101010;
    cursor: pointer;
    font: 500 11px var(--serif);
    transition: color 0.4s var(--ease-soft), border-color 0.4s var(--ease-soft), background-color 0.4s var(--ease-soft), box-shadow 0.6s var(--ease-soft), transform 0.4s var(--ease-out);
  }
  .action:hover:not(:disabled) {
    color: var(--text);
    border-color: #3d3d3d;
    background: #161616;
    transform: translateY(-1px);
    box-shadow: 0 0 18px rgba(255, 255, 255, 0.05);
  }
  .action.primary {
    color: var(--inverse);
    border-color: var(--text);
    background: var(--text);
    font-weight: 600;
  }
  .action.primary:hover:not(:disabled) { background: #fff; box-shadow: 0 0 26px rgba(255, 255, 255, 0.18); }
  .action.stop { color: #d9b0b0; border-color: #472e2e; background: #170f0f; }
  .action.live { color: var(--text); border-color: #4a4a4a; }
  .live-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
    box-shadow: 0 0 8px rgba(255, 255, 255, 0.6);
  }
  .action.live .live-dot { animation: toolbarPulse 1.6s ease-in-out infinite; }
  @keyframes toolbarPulse {
    0%, 100% { opacity: 0.4; }
    50% { opacity: 1; }
  }
  .toolbar-meta {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-left: auto;
  }
  .status-message, .frame-meta {
    overflow: hidden;
    color: var(--muted);
    font: 500 11px var(--mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .status-message.error { color: #d9a8a8; }
  .frame-meta.dim { color: var(--dim); }
  .meta-button {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border: 1px solid transparent;
    border-radius: 9px;
    color: var(--muted);
    background: transparent;
    cursor: pointer;
    transition: color 0.4s var(--ease-soft), border-color 0.4s var(--ease-soft), background-color 0.4s var(--ease-soft);
  }
  .meta-button:hover, .meta-button.active {
    color: var(--text);
    border-color: var(--border);
    background: rgba(255, 255, 255, 0.045);
  }
  .market-body {
    display: grid;
    grid-template-columns: minmax(0, 1.72fr) minmax(340px, 1fr);
    gap: 14px;
    min-height: 0;
    padding: 14px 18px;
  }
  .chart-region { position: relative; min-width: 0; min-height: 0; }
  .chart-placeholder {
    display: grid;
    place-content: center;
    gap: 10px;
    height: 100%;
    border: 1px dashed var(--border-soft);
    border-radius: var(--radius-md);
    color: var(--dim);
    text-align: center;
  }
  .chart-placeholder p { margin: 0; font-size: 12px; }
  .market-side {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 10px;
    min-width: 0;
    min-height: 0;
  }
  .market-side > :global(.settings-panel),
  .market-side > :global(.records-panel) {
    grid-row: 1 / -1;
  }
  .side-tabs {
    display: flex;
    gap: 4px;
    padding: 4px;
    border: 1px solid var(--border-soft);
    border-radius: var(--radius-md);
    background: var(--surface-1);
    overflow-x: auto;
    scrollbar-width: none;
  }
  .side-tabs button {
    flex: 0 0 auto;
    height: 28px;
    padding: 0 11px;
    border: 0;
    border-radius: 9px;
    color: var(--muted);
    background: transparent;
    cursor: pointer;
    font: 500 11px var(--serif);
    white-space: nowrap;
    transition: color 0.4s var(--ease-soft), background-color 0.4s var(--ease-soft), box-shadow 0.5s var(--ease-soft);
  }
  .side-tabs button:hover { color: var(--text-soft); }
  .side-tabs button.active {
    color: var(--text);
    background: #1d1d1d;
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.045), 0 0 14px rgba(255, 255, 255, 0.04);
  }
  .side-body { min-height: 0; }
  .side-body-inner {
    height: 100%;
    min-height: 0;
    animation: sidePanelIn 0.42s var(--ease-out) both;
  }
  @keyframes sidePanelIn {
    from { opacity: 0; transform: translateY(8px); }
    to { opacity: 1; transform: none; }
  }
  .market-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    padding: 8px 18px;
    border-top: 1px solid var(--border-soft);
    color: var(--dim);
    font: 10px var(--serif);
  }
  .foot-error { overflow: hidden; color: #cf9d9d; text-overflow: ellipsis; white-space: nowrap; }
</style>
