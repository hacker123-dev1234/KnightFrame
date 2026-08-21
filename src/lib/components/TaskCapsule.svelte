<script lang="ts">
  import Icon from './Icon.svelte';
  import type { Locale, SessionSnapshot } from '../types';
  import { translate } from '../i18n';
  import { taskCapsuleSummary } from '../taskSummary';

  export let locale: Locale;
  export let session: SessionSnapshot;
  export let panelOpen = false;
  export let onOpen: () => void;

  $: t = (key: string, args: Record<string, string | number> = {}) => translate(locale, key, args);
  $: summary = taskCapsuleSummary(session);
  $: hasLineStats = summary?.additions !== undefined && summary?.deletions !== undefined
    && (summary.additions > 0 || summary.deletions > 0);
</script>

{#if summary}
  <button
    class:running={summary.status === 'running' || summary.status === 'pending'}
    class:open={panelOpen}
    class="task-capsule"
    type="button"
    on:click={onOpen}
    aria-expanded={panelOpen}
    aria-label={`${t('task.capsule.step', { step: summary.step, total: summary.total })}: ${t(summary.current)}`}
  >
    <span class="task-capsule-sigil"><Icon name="tasks" size={15} /></span>
    <span class="task-capsule-copy">
      <small>{t('task.capsule.step', { step: summary.step, total: summary.total })}</small>
      <strong>{t(summary.current)}</strong>
    </span>
    <span class="task-capsule-stats">
      {#if summary.filesChanged !== undefined && summary.filesChanged > 0}<span>{t('task.capsule.files', { count: summary.filesChanged })}</span>{/if}
      {#if hasLineStats}<span class="task-additions">+{summary.additions}</span><span class="task-deletions">-{summary.deletions}</span>{/if}
    </span>
    <span class="task-capsule-chevron"><Icon name="chevron" size={14} /></span>
  </button>
{/if}
