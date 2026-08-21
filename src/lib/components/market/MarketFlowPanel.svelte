<script lang="ts">
  import { onDestroy } from 'svelte';
  import Icon from '../Icon.svelte';
  import { translate } from '../../i18n';
  import { extractTrace } from '../../market-utils';
  import type { AnalysisRecord, Locale } from '../../types';

  export let locale: Locale;
  export let record: AnalysisRecord | undefined;
  export let autoPlay: boolean;
  export let playSeconds: number;

  $: t = (key: string, args: Record<string, string | number> = {}) => translate(locale, key, args);
  $: nodes = extractTrace(record);
  $: intervalMs = Math.max(120, (playSeconds * 1000) / Math.max(1, nodes.length));

  let cursor = 0;
  let playing = false;
  let timer: ReturnType<typeof setInterval> | undefined;

  function reset() {
    cursor = 0;
    playing = false;
    stopTimer();
  }

  function stopTimer() {
    if (timer) {
      clearInterval(timer);
      timer = undefined;
    }
  }

  function play() {
    if (!nodes.length) return;
    if (cursor >= nodes.length) cursor = 0;
    playing = true;
    stopTimer();
    timer = setInterval(() => {
      cursor += 1;
      if (cursor >= nodes.length) {
        playing = false;
        stopTimer();
      }
    }, intervalMs);
  }

  function pause() {
    playing = false;
    stopTimer();
  }

  function step() {
    pause();
    if (cursor < nodes.length) cursor += 1;
  }

  // 新纪录到达时重置并按设置自动播放
  $: if (record) {
    reset();
    if (autoPlay && nodes.length) play();
  }

  $: activeNode = nodes[cursor];
  $: progress = nodes.length ? Math.round(((cursor + 1) / nodes.length) * 100) : 0;

  onDestroy(stopTimer);
</script>

<div class="flow-panel">
  {#if !nodes.length}
    <div class="flow-empty">
      <Icon name="play" size={24} />
      <p>{t('market.flow.empty')}</p>
    </div>
  {:else}
    <div class="flow-controls">
      {#if playing}
        <button type="button" on:click={pause} aria-label={t('market.flow.pause')} title={t('market.flow.pause')}>
          <Icon name="pause" size={15} />
        </button>
      {:else}
        <button type="button" on:click={play} aria-label={t('market.flow.play')} title={t('market.flow.play')}>
          <Icon name="play" size={15} />
        </button>
      {/if}
      <button type="button" on:click={step} aria-label={t('market.flow.step')} title={t('market.flow.step')}>
        <Icon name="forward" size={15} />
      </button>
      <button type="button" on:click={reset} aria-label={t('market.flow.reset')} title={t('market.flow.reset')}>
        <Icon name="refresh" size={15} />
      </button>
      <span class="flow-position">{t('market.flow.node', { index: Math.min(cursor + 1, nodes.length), total: nodes.length })}</span>
      <span class="flow-progress" style={`--progress:${progress}%`}></span>
    </div>

    <div class="flow-scroll">
      <div class="flow-timeline">
        {#each nodes as node, index (node.nodeId)}
          <div
            class="flow-step"
            class:lit={index <= cursor}
            class:active={index === cursor}
            class:skipped={node.skipped}
            class:answer-yes={node.answer === '是'}
            class:answer-no={node.answer === '否'}
            style={`--delay:${Math.min(index, 24) * 40}ms`}
          >
            <span class="step-dot"></span>
            <div class="step-body">
              <header>
                <span class="step-id">{node.nodeId}</span>
                <span class="step-answer">{node.answer}</span>
              </header>
              {#if node.question}<p class="step-question">{node.question}</p>{/if}
              {#if node.reason && index <= cursor}<p class="step-reason">{node.reason}</p>{/if}
            </div>
          </div>
        {/each}
      </div>
    </div>

    {#if activeNode}
      <footer class="flow-footer">
        <span class="footer-id">{activeNode.nodeId}</span>
        <span class="footer-question">{activeNode.question || activeNode.reason}</span>
      </footer>
    {/if}
  {/if}
</div>

<style>
  .flow-panel {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    gap: 10px;
    width: 100%;
    height: 100%;
    min-height: 0;
  }
  .flow-empty {
    display: grid;
    place-items: center;
    gap: 9px;
    height: 100%;
    color: var(--dim);
    font-size: 12px;
    text-align: center;
  }
  .flow-empty p { margin: 0; }
  .flow-controls {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 8px;
    border: 1px solid var(--border-soft);
    border-radius: var(--radius-sm);
    background: var(--surface-1);
  }
  .flow-controls button {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 9px;
    color: var(--muted);
    background: transparent;
    cursor: pointer;
    transition: color 0.4s var(--ease-soft), background-color 0.4s var(--ease-soft), transform 0.4s var(--ease-out);
  }
  .flow-controls button:hover { color: var(--text); background: rgba(255, 255, 255, 0.055); transform: translateY(-1px); }
  .flow-position {
    margin-left: auto;
    color: var(--dim);
    font: 500 10px var(--mono);
    letter-spacing: 0.06em;
  }
  .flow-progress {
    width: 74px;
    height: 2px;
    border-radius: 2px;
    background: rgba(255, 255, 255, 0.08);
    overflow: hidden;
  }
  .flow-progress::after {
    display: block;
    width: var(--progress);
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg, rgba(255, 255, 255, 0.45), #fff);
    box-shadow: 0 0 8px rgba(255, 255, 255, 0.45);
    transition: width 0.4s var(--ease-out);
  }
  .flow-scroll {
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: #2d2d2d transparent;
  }
  .flow-timeline {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding-left: 15px;
  }
  .flow-timeline::before {
    content: '';
    position: absolute;
    top: 8px;
    bottom: 8px;
    left: 3px;
    width: 1px;
    background: var(--border-soft);
  }
  .flow-step {
    position: relative;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    padding: 7px 10px;
    border: 1px solid transparent;
    border-radius: 10px;
    opacity: 0.28;
    transition: opacity 0.45s var(--ease-soft), border-color 0.45s var(--ease-soft), background-color 0.45s var(--ease-soft), transform 0.5s var(--ease-out);
  }
  .flow-step.lit { opacity: 1; }
  .flow-step.active {
    border-color: var(--border);
    background: #131313;
    transform: translateX(3px);
  }
  .flow-step.skipped .step-answer { text-decoration: line-through; }
  .step-dot {
    position: absolute;
    top: 13px;
    left: -15px;
    width: 7px;
    height: 7px;
    border: 1px solid #4a4a4a;
    border-radius: 50%;
    background: #0a0a0a;
    transition: border-color 0.45s var(--ease-soft), box-shadow 0.55s var(--ease-soft), background-color 0.45s var(--ease-soft);
  }
  .flow-step.lit .step-dot { border-color: rgba(255, 255, 255, 0.7); }
  .flow-step.answer-yes.lit .step-dot { background: #d9d9d9; box-shadow: 0 0 9px rgba(255, 255, 255, 0.55); }
  .flow-step.active .step-dot { box-shadow: 0 0 12px rgba(255, 255, 255, 0.75); }
  .step-body header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .step-id { color: var(--text); font: 600 11px var(--mono); }
  .step-answer {
    padding: 1px 7px;
    border: 1px solid var(--border-soft);
    border-radius: 999px;
    color: var(--text-soft);
    font: 500 10px var(--serif);
  }
  .step-question { margin: 4px 0 0; color: var(--text-soft); font: 400 12px/1.55 var(--serif); }
  .step-reason { margin: 3px 0 0; color: var(--muted); font: 10px/1.6 var(--mono); white-space: pre-wrap; overflow-wrap: anywhere; }
  .flow-footer {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 8px 11px;
    border: 1px solid var(--border-soft);
    border-radius: var(--radius-sm);
    background: var(--surface-1);
  }
  .footer-id { color: var(--text); font: 600 11px var(--mono); }
  .footer-question {
    overflow: hidden;
    color: var(--muted);
    font: 11px/1.5 var(--serif);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
