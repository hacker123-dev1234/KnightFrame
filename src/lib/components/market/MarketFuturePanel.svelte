<script lang="ts">
  import Icon from '../Icon.svelte';
  import { translate } from '../../i18n';
  import type { AnalysisRecord, Locale } from '../../types';

  export let locale: Locale;
  export let record: AnalysisRecord | undefined;

  $: t = (key: string) => translate(locale, key);
  $: stage2 = record?.stage2Decision as Record<string, unknown> | undefined;
  $: barPrediction = stage2?.next_bar_prediction as Record<string, unknown> | undefined;
  $: cyclePrediction = stage2?.next_cycle_prediction as Record<string, unknown> | undefined;
  $: barProbs = (barPrediction?.probabilities ?? {}) as Record<string, number>;
  $: cycleProbs = (cyclePrediction?.probabilities ?? {}) as Record<string, number>;
  $: hasAny = Boolean(barPrediction || cyclePrediction);
</script>

<div class="future-panel">
  {#if !hasAny}
    <div class="future-empty">
      <Icon name="clock" size={26} />
      <p>{t('market.future.empty')}</p>
    </div>
  {:else}
    <div class="future-scroll">
      {#if barPrediction}
        <section class="future-block">
          <header>
            <h4>next_bar_prediction</h4>
            {#if barPrediction.direction}
              <span class="dir dir-{String(barPrediction.direction)}">{String(barPrediction.direction)}</span>
            {/if}
            {#if barPrediction.unpredictable === true}<span class="unpredictable">unpredictable</span>{/if}
          </header>
          {#if Object.keys(barProbs).length}
            <div class="prob-bars">
              {#each Object.entries(barProbs).slice(0, 3) as [key, value] (key)}
                <div class="prob-row">
                  <span class="prob-key">{key}</span>
                  <span class="prob-track"><i style={`--p:${Math.max(0, Math.min(100, Number(value) || 0))}%`}></i></span>
                  <span class="prob-value">{Number(value) ?? 0}%</span>
                </div>
              {/each}
            </div>
          {/if}
          {#if typeof barPrediction.reasoning === 'string'}
            <p class="future-reasoning">{barPrediction.reasoning}</p>
          {/if}
        </section>
      {/if}

      {#if cyclePrediction}
        <section class="future-block">
          <header>
            <h4>next_cycle_prediction</h4>
            {#if cyclePrediction.cycle}<span class="dir">{String(cyclePrediction.cycle)}</span>{/if}
            {#if cyclePrediction.unpredictable === true}<span class="unpredictable">unpredictable</span>{/if}
          </header>
          {#if Object.keys(cycleProbs).length}
            <div class="prob-bars dense">
              {#each Object.entries(cycleProbs).sort((a, b) => (Number(b[1]) || 0) - (Number(a[1]) || 0)).slice(0, 4) as [key, value] (key)}
                <div class="prob-row">
                  <span class="prob-key">{key}</span>
                  <span class="prob-track"><i style={`--p:${Math.max(0, Math.min(100, Number(value) || 0))}%`}></i></span>
                  <span class="prob-value">{Number(value) ?? 0}%</span>
                </div>
              {/each}
            </div>
          {/if}
          {#if typeof cyclePrediction.reasoning === 'string'}
            <p class="future-reasoning">{cyclePrediction.reasoning}</p>
          {/if}
        </section>
      {/if}
    </div>
  {/if}
</div>

<style>
  .future-panel {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    min-height: 0;
  }
  .future-empty {
    display: grid;
    place-items: center;
    gap: 9px;
    height: 100%;
    color: var(--dim);
    font-size: 12px;
    text-align: center;
  }
  .future-empty p { margin: 0; }
  .future-scroll {
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: #2d2d2d transparent;
  }
  .future-block {
    margin-bottom: 12px;
    padding: 12px 13px;
    border: 1px solid var(--border-soft);
    border-radius: var(--radius-md);
    background: var(--surface-1);
  }
  .future-block header {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-bottom: 10px;
  }
  .future-block h4 {
    margin: 0;
    color: var(--dim);
    font: 600 9px var(--mono);
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }
  .dir {
    padding: 2px 8px;
    border: 1px solid var(--border);
    border-radius: 999px;
    color: var(--text-soft);
    font: 500 10px var(--serif);
  }
  .dir-bullish { border-color: #3d4a3f; color: #b9d0be; }
  .dir-bearish { border-color: #4a3d3d; color: #cbb3b3; }
  .unpredictable {
    padding: 2px 8px;
    border: 1px dashed var(--border-strong);
    border-radius: 999px;
    color: var(--muted);
    font: 500 9px var(--mono);
  }
  .prob-bars { display: grid; gap: 7px; margin-bottom: 10px; }
  .prob-bars.dense { gap: 5px; }
  .prob-row {
    display: grid;
    grid-template-columns: 74px minmax(0, 1fr) 44px;
    align-items: center;
    gap: 9px;
  }
  .prob-key {
    overflow: hidden;
    color: var(--muted);
    font: 500 10px var(--mono);
    text-overflow: ellipsis;
  }
  .prob-track {
    height: 3px;
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.07);
    overflow: hidden;
  }
  .prob-track i {
    display: block;
    width: var(--p);
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg, rgba(255, 255, 255, 0.4), rgba(255, 255, 255, 0.9));
    box-shadow: 0 0 7px rgba(255, 255, 255, 0.35);
    transition: width 0.7s var(--ease-out);
  }
  .prob-value { color: var(--text-soft); font: 500 10px var(--mono); text-align: right; }
  .future-reasoning {
    margin: 0;
    color: var(--muted);
    font: 11px/1.7 var(--serif);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
