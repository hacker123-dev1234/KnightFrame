<script lang="ts">
  import { translate } from '../../i18n';
  import Select from '../Select.svelte';
  import type { Locale, MarketSettings } from '../../types';

  export let locale: Locale;
  export let settings: MarketSettings | undefined;
  export let onSave: (settings: MarketSettings) => Promise<void>;

  let draft: MarketSettings | undefined;
  let saving = false;
  let saved = false;

  $: t = (key: string) => translate(locale, key);
  // 只在首次拿到 settings 时初始化草稿。不能写 `draft !== settings`：
  // 该引用比较恒为真，bind:value 触发组件更新后草稿会被立刻重置，
  // 表现为所有下拉/输入"选不上"。
  $: if (settings && !draft) draft = structuredClone(settings);
  $: dirty = Boolean(draft && JSON.stringify(draft) !== JSON.stringify(settings));

  const stances = ['conservative', 'balanced', 'aggressive', 'extreme_aggressive'];
  const timeframes = ['1m', '5m', '15m', '30m', '1h', '4h', '1d'];
  const tvExchanges = ['OANDA', 'FX', 'BINANCE', 'NASDAQ', 'NYSE', 'COMEX'];

  async function save() {
    if (!draft) return;
    saving = true;
    try {
      await onSave(draft);
      saved = true;
      setTimeout(() => (saved = false), 1600);
    } finally {
      saving = false;
    }
  }
</script>

<div class="settings-panel">
  {#if draft}
    <div class="settings-scroll">
      <section class="settings-group">
        <h4>{t('market.settings.provider')}</h4>
        <label class="field">
          <span>{t('market.settings.model')}</span>
          <input type="text" bind:value={draft.provider.model} />
        </label>
        <label class="field">
          <span>{t('market.settings.baseUrl')}</span>
          <input type="text" bind:value={draft.provider.baseUrl} />
        </label>
        <label class="field">
          <span>{t('market.settings.apiKey')}</span>
          <input type="password" bind:value={draft.provider.apiKey} />
        </label>
        <div class="field-row">
          <label class="field">
            <span>{t('market.settings.thinking')}</span>
            <input type="checkbox" bind:checked={draft.provider.thinking} />
          </label>
          <label class="field">
            <span>{t('market.settings.effort')}</span>
            <Select bind:value={draft.provider.reasoningEffort} options={['low', 'medium', 'high', 'max'].map((item) => ({ value: item, label: item }))} />
          </label>
          <label class="field">
            <span>{t('market.settings.contextWindow')}</span>
            <input type="number" min="4096" step="4096" bind:value={draft.provider.contextWindow} />
          </label>
        </div>
      </section>

      <section class="settings-group">
        <h4>{t('market.settings.general')}</h4>
        <div class="field-row">
          <label class="field">
            <span>{t('market.settings.barCount')}</span>
            <input type="number" min="20" max="5000" bind:value={draft.general.analysisBarCount} />
          </label>
          <label class="field">
            <span>{t('market.settings.refreshInterval')}</span>
            <input type="number" min="250" step="250" bind:value={draft.general.refreshIntervalMs} />
          </label>
          <label class="field">
            <span>{t('market.settings.incrementalMax')}</span>
            <input type="number" min="0" max="200" bind:value={draft.general.incrementalMaxNewBars} />
          </label>
        </div>
        <div class="field-row">
          <label class="field">
            <span>{t('market.settings.stance')}</span>
            <Select bind:value={draft.general.decisionStance} options={stances.map((stance) => ({ value: stance, label: t(`market.settings.stance.${stance}`) }))} />
          </label>
          <label class="field">
            <span>{t('market.exchange')} (TradingView)</span>
            <Select bind:value={draft.general.lastTradingviewExchange} options={tvExchanges.map((exchange) => ({ value: exchange, label: exchange }))} />
          </label>
          <label class="field">
            <span>{t('market.timeframe')}</span>
            <Select bind:value={draft.general.lastTimeframe} options={timeframes.map((timeframe) => ({ value: timeframe, label: timeframe }))} />
          </label>
        </div>
      </section>

      <section class="settings-group">
        <h4>{t('market.settings.prompt')}</h4>
        <div class="field-row">
          <label class="field">
            <span>{t('market.settings.experienceEntries')}</span>
            <input type="number" min="0" max="20" bind:value={draft.prompt.experienceMaxEntries} />
          </label>
          <label class="field">
            <span>{t('market.settings.experienceChars')}</span>
            <input type="number" min="80" step="40" bind:value={draft.prompt.experienceMaxCharsPerEntry} />
          </label>
        </div>
        <label class="field inline">
          <input type="checkbox" bind:checked={draft.prompt.stage2LoadFullStrategyLibrary} />
          <span>{t('market.settings.stage2FullLibrary')}</span>
        </label>
        <label class="field inline">
          <input type="checkbox" bind:checked={draft.prompt.stage1InjectPatternBriefs} />
          <span>{t('market.settings.stage1Patterns')}</span>
        </label>
      </section>

      <section class="settings-group">
        <h4>{t('market.settings.validation')}</h4>
        <label class="field">
          <span>{t('market.settings.normalization')}</span>
          <Select bind:value={draft.validation.normalizationMode} options={['strict', 'lenient'].map((mode) => ({ value: mode, label: t(`market.settings.normalization.${mode}`) }))} />
        </label>
        <div class="field-row">
          <label class="field">
            <span>{t('market.settings.retryMax')}</span>
            <input type="number" min="0" max="6" bind:value={draft.validation.retryMax} />
          </label>
          <label class="field">
            <span>{t('market.settings.retryMaxSemantic')}</span>
            <input type="number" min="0" max="4" bind:value={draft.validation.retryMaxSemantic} />
          </label>
        </div>
        <label class="field inline">
          <input type="checkbox" bind:checked={draft.validation.retryEnabled} />
          <span>{t('market.settings.retryEnabled')}</span>
        </label>
        <label class="field inline">
          <input type="checkbox" bind:checked={draft.validation.retryStage2} />
          <span>{t('market.settings.retryStage2')}</span>
        </label>
      </section>
    </div>

    <footer class="settings-footer">
      <button type="button" class="save-button" disabled={!dirty || saving} on:click={save}>
        {saving ? '…' : saved ? t('market.settings.saved') : t('market.settings.save')}
      </button>
    </footer>
  {/if}
</div>

<style>
  .settings-panel {
    display: grid;
    grid-template-rows: minmax(0, 1fr) auto;
    gap: 10px;
    width: 100%;
    height: 100%;
    min-height: 0;
  }
  .settings-scroll {
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: #2d2d2d transparent;
  }
  .settings-group {
    margin-bottom: 12px;
    padding: 12px 13px;
    border: 1px solid var(--border-soft);
    border-radius: var(--radius-md);
    background: var(--surface-1);
  }
  .settings-group h4 {
    margin: 0 0 10px;
    color: var(--dim);
    font: 600 9px var(--mono);
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }
  .field { display: grid; gap: 5px; min-width: 0; }
  .field > span { color: var(--muted); font: 500 10px var(--serif); }
  .field input[type='text'],
  .field input[type='password'],
  .field input[type='number'] {
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
  .field input:focus {
    border-color: rgba(255, 255, 255, 0.26);
    box-shadow: 0 0 18px rgba(255, 255, 255, 0.05);
  }
  .field-row { display: grid; grid-template-columns: repeat(auto-fit, minmax(110px, 1fr)); gap: 9px; margin-bottom: 9px; }
  .field-row:last-child { margin-bottom: 0; }
  .field.inline { display: flex; align-items: center; gap: 8px; margin-top: 8px; }
  .field.inline input { width: 14px; height: 14px; accent-color: #ddd; }
  .field.inline span { font-size: 11px; }
  .settings-footer { display: flex; justify-content: flex-end; }
  .save-button {
    height: 34px;
    padding: 0 18px;
    border: 0;
    border-radius: 11px;
    color: var(--inverse);
    background: #eee;
    cursor: pointer;
    font: 600 12px var(--serif);
    transition: transform 0.4s var(--ease-out), box-shadow 0.6s var(--ease-soft), background-color 0.4s var(--ease-soft), opacity 0.4s var(--ease-soft);
  }
  .save-button:hover:not(:disabled) { transform: translateY(-1px); background: #fff; box-shadow: 0 0 22px rgba(255, 255, 255, 0.16); }
  .save-button:disabled { opacity: 0.4; cursor: not-allowed; }
</style>
