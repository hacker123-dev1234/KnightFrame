<script lang="ts">
  import Icon from './Icon.svelte';
  import StatusMark from './StatusMark.svelte';
  import ToolCard from './ToolCard.svelte';
  import type { Locale, ReceiptStatus, ToolReceipt } from '../types';
  import { duration, translate } from '../i18n';

  export let locale: Locale;
  export let tools: ToolReceipt[];

  let expanded = false;
  $: t = (key: string, args: Record<string, string | number> = {}) => translate(locale, key, args);
  $: status = aggregateStatus(tools);
  $: failedCount = tools.filter((tool) => tool.status === 'failed').length;
  $: elapsedMs = tools.some((tool) => tool.elapsedMs !== undefined)
    ? tools.reduce((total, tool) => total + (tool.elapsedMs ?? 0), 0)
    : undefined;
  $: summary = t('workflow.summary', { count: tools.length });
  $: detailsId = `workflow-details-${tools[0]?.id ?? 'tools'}`;

  function aggregateStatus(receipts: ToolReceipt[]): ReceiptStatus {
    if (receipts.some((tool) => tool.status === 'running')) return 'running';
    if (receipts.some((tool) => tool.status === 'pending')) return 'pending';
    if (receipts.some((tool) => tool.status === 'failed')) return 'failed';
    if (receipts.some((tool) => tool.status === 'blocked')) return 'blocked';
    if (receipts.some((tool) => tool.status === 'cancelled')) return 'cancelled';
    if (receipts.every((tool) => tool.status === 'skipped')) return 'skipped';
    return 'completed';
  }
</script>

<section class:running={status === 'running'} class:failed={status === 'failed'} class:expanded class="workflow-group">
  <button
    class="workflow-heading"
    type="button"
    on:click={() => expanded = !expanded}
    aria-expanded={expanded}
    aria-controls={detailsId}
    aria-label={`${t(expanded ? 'workflow.collapse' : 'workflow.expand')}: ${summary}`}
  >
    <span class="workflow-icon"><Icon name="graph" size={16} /></span>
    <span class="workflow-name">
      <small>{t('workflow.title')}</small>
      <strong>{summary}</strong>
      {#if failedCount > 0}<span>{t('workflow.failures', { count: failedCount })}</span>{/if}
    </span>
    {#if elapsedMs !== undefined}<span class="workflow-time">{duration(locale, elapsedMs)}</span>{/if}
    <StatusMark {status} label={t(`tool.state.${status}`)} />
    <span class:open={expanded} class="workflow-chevron"><Icon name="chevron" size={14} /></span>
  </button>

  {#if expanded}
    <div class="workflow-body" id={detailsId}>
      {#each tools as tool (tool.id)}
        <div class="workflow-step"><ToolCard {locale} {tool} /></div>
      {/each}
    </div>
  {/if}
</section>
