<script lang="ts">
  import Icon from '../Icon.svelte';
  import { translate } from '../../i18n';
  import { extractTrace } from '../../market-utils';
  import type { AnalysisRecord, Locale } from '../../types';

  export let locale: Locale;
  export let record: AnalysisRecord | undefined;

  $: t = (key: string) => translate(locale, key);
  $: nodes = extractTrace(record);
  $: terminal = (record?.stage1Diagnosis as Record<string, unknown> | undefined)?.terminal as
    | Record<string, unknown>
    | undefined;
  $: gateResult = (record?.stage1Diagnosis as Record<string, unknown> | undefined)?.gate_result;
</script>

<div class="tree-panel">
  {#if !nodes.length}
    <div class="tree-empty">
      <Icon name="market" size={26} />
      <p>{t('market.tree.empty')}</p>
    </div>
  {:else}
    <div class="tree-scroll">
      {#if gateResult}
        <div class="tree-gate gate-{gateResult}">
          <span class="gate-key">gate_result</span>
          <strong>{gateResult}</strong>
        </div>
      {/if}
      {#each nodes as node (node.nodeId)}
        <article
          class="tree-node"
          class:skipped={node.skipped}
          style={`--depth:${node.depth}`}
          class:answer-yes={node.answer === '是'}
          class:answer-no={node.answer === '否'}
        >
          <header>
            <span class="node-id">{node.nodeId}</span>
            {#if node.section}<span class="node-section">{node.section}</span>{/if}
            {#if node.barRange}<span class="node-range">{node.barRange}</span>{/if}
            <span class="node-answer">{node.answer}</span>
          </header>
          {#if node.question}<p class="node-question">{node.question}</p>{/if}
          {#if node.reason}<p class="node-reason">{node.reason}</p>{/if}
          {#if node.branch}<p class="node-branch">→ {node.branch}</p>{/if}
        </article>
      {/each}
      {#if terminal}
        <article class="tree-node terminal">
          <header>
            <span class="node-id">{terminal.node_id ?? 'T'}</span>
            <span class="node-answer">{terminal.outcome ?? terminal.label ?? ''}</span>
          </header>
          {#if terminal.label && terminal.outcome}<p class="node-reason">{terminal.label}</p>{/if}
        </article>
      {/if}
    </div>
  {/if}
</div>

<style>
  .tree-panel {
    display: flex;
    flex-direction: column;
    width: 100%;
    height: 100%;
    min-height: 0;
  }
  .tree-empty {
    display: grid;
    place-items: center;
    gap: 9px;
    height: 100%;
    color: var(--dim);
    font-size: 12px;
    text-align: center;
  }
  .tree-empty p { margin: 0; }
  .tree-scroll {
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: #2d2d2d transparent;
  }
  .tree-gate {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 12px;
    padding: 9px 12px;
    border: 1px solid var(--border-soft);
    border-radius: var(--radius-sm);
    background: var(--surface-1);
  }
  .tree-gate .gate-key {
    color: var(--dim);
    font: 500 10px var(--mono);
    letter-spacing: 0.08em;
  }
  .tree-gate strong { font: 600 12px var(--serif); }
  .gate-proceed strong { color: #b9d0be; }
  .gate-wait strong { color: #c9b58e; }
  .tree-node {
    margin: 0 0 8px calc(var(--depth) * 14px);
    padding: 9px 11px;
    border: 1px solid var(--border-soft);
    border-left: 2px solid #3a3a3a;
    border-radius: 10px;
    background: var(--surface-1);
    transition: border-color 0.4s var(--ease-soft), background-color 0.4s var(--ease-soft);
  }
  .tree-node:hover { background: #121212; }
  .tree-node.answer-yes { border-left-color: #cfdcd2; }
  .tree-node.answer-no { border-left-color: #6f6f6f; }
  .tree-node.skipped { opacity: 0.45; }
  .tree-node.terminal { border-left-color: var(--text); background: #141414; }
  .tree-node header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 4px;
  }
  .node-id {
    color: var(--text);
    font: 600 11px var(--mono);
    letter-spacing: 0.04em;
  }
  .node-section, .node-range {
    color: var(--dim);
    font: 10px var(--mono);
  }
  .node-answer {
    margin-left: auto;
    padding: 2px 7px;
    border: 1px solid var(--border-soft);
    border-radius: 999px;
    color: var(--text-soft);
    font: 500 10px var(--serif);
  }
  .node-question {
    margin: 0 0 3px;
    color: var(--text-soft);
    font: 400 12px/1.6 var(--serif);
  }
  .node-reason {
    margin: 0;
    color: var(--muted);
    font: 11px/1.65 var(--serif);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .node-branch {
    margin: 3px 0 0;
    color: #8d9fa0;
    font: 10px var(--mono);
  }
</style>
