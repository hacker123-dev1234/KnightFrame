<script lang="ts">
  import Icon from './Icon.svelte';
  import StatusMark from './StatusMark.svelte';
  import type { AuxiliaryReceipt, Locale } from '../types';
  import { duration, translate } from '../i18n';

  export let locale: Locale;
  export let receipt: AuxiliaryReceipt;

  let expanded = false;
  $: t = (key: string) => translate(locale, key);
  $: hasDetails = Boolean(receipt.summary || receipt.reason || receipt.inputTokens !== undefined || receipt.outputTokens !== undefined);
  $: role = t(`auxiliary.role.${receipt.role}`);
  $: status = t(`auxiliary.status.${receipt.status}`);
</script>

<section class:running={receipt.status === 'running'} class:failed={receipt.status === 'failed'} class="auxiliary-card">
  <button
    class:static={!hasDetails}
    type="button"
    on:click={() => hasDetails && (expanded = !expanded)}
    aria-expanded={hasDetails ? expanded : undefined}
    aria-controls={hasDetails ? `auxiliary-details-${receipt.id}` : undefined}
  >
    <span class="auxiliary-icon"><Icon name="spark" size={15} /></span>
    <span class="auxiliary-name"><small>{t('auxiliary.title')}</small><strong>{role}</strong></span>
    {#if receipt.beforeTokens !== undefined}
      <span class="auxiliary-saving">{receipt.beforeTokens}{#if receipt.afterTokens !== undefined} → {receipt.afterTokens}{/if}</span>
    {/if}
    <StatusMark status={receipt.status} label={status} />
    <span class:open={expanded} class:hidden={!hasDetails} class="auxiliary-chevron"><Icon name="chevron" size={14} /></span>
  </button>
  {#if expanded && hasDetails}
    <div class="auxiliary-body" id={`auxiliary-details-${receipt.id}`}>
      <dl>
        {#if receipt.model}<div><dt>{t('auxiliary.model')}</dt><dd>{receipt.model}</dd></div>{/if}
        {#if receipt.elapsedMs !== undefined}<div><dt>{t('auxiliary.duration')}</dt><dd>{duration(locale, receipt.elapsedMs)}</dd></div>{/if}
        {#if receipt.inputTokens !== undefined}<div><dt>{t('auxiliary.input')}</dt><dd>{receipt.inputTokens}</dd></div>{/if}
        {#if receipt.outputTokens !== undefined}<div><dt>{t('auxiliary.output')}</dt><dd>{receipt.outputTokens}</dd></div>{/if}
      </dl>
      {#if receipt.reason}<p class="auxiliary-reason">{t(`auxiliary.reason.${receipt.reason}`)}</p>{/if}
      {#if receipt.summary}<section><h4>{t('auxiliary.brief')}</h4><p>{receipt.summary}</p></section>{/if}
    </div>
  {/if}
</section>
