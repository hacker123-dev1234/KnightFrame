<script lang="ts">
  import Icon from './Icon.svelte';
  import type { Locale, UsageSnapshot } from '../types';
  import { duration, money, number, translate } from '../i18n';

  export let locale: Locale;
  export let usage: UsageSnapshot | undefined;
  export let onClose: () => void;

  $: t = (key: string) => translate(locale, key);
  $: total = usage
    ? (usage.freshInputTokens ?? 0) + (usage.cacheReadTokens ?? 0) + (usage.cacheWriteTokens ?? 0) + (usage.outputTokens ?? 0)
    : undefined;
  $: turnCacheTotal = (usage?.turnFreshInputTokens ?? 0) + (usage?.turnCacheReadTokens ?? 0);
  $: sessionCacheTotal = (usage?.freshInputTokens ?? 0) + (usage?.cacheReadTokens ?? 0);
  $: turnCacheHit = turnCacheTotal > 0 ? Math.round(((usage?.turnCacheReadTokens ?? 0) / turnCacheTotal) * 100) : undefined;
  $: sessionCacheHit = sessionCacheTotal > 0 ? Math.round(((usage?.cacheReadTokens ?? 0) / sessionCacheTotal) * 100) : undefined;
</script>

<aside class="inspector usage-inspector" aria-label={t('usage.title')}>
  <header>
    <div><span class="inspector-eyebrow">{t('usage.title')}</span><h2>{number(locale, total)}</h2></div>
    <button type="button" on:click={onClose} aria-label={t('app.close')}><Icon name="close" /></button>
  </header>
  <div class="usage-hero">
    <div><span>{t('usage.turnTime')}</span><strong>{duration(locale, usage?.turnElapsedMs)}</strong></div>
    <div><span>{t('usage.sessionTime')}</span><strong>{duration(locale, usage?.sessionElapsedMs)}</strong></div>
  </div>
  <dl class="usage-grid">
    <div><dt>{t('usage.cacheHitCurrent')}</dt><dd>{turnCacheHit === undefined ? t('usage.unavailable') : `${turnCacheHit}%`}</dd></div>
    <div><dt>{t('usage.cacheHitSession')}</dt><dd>{sessionCacheHit === undefined ? t('usage.unavailable') : `${sessionCacheHit}%`}</dd></div>
    <div><dt>{t('usage.freshInput')}</dt><dd>{number(locale, usage?.freshInputTokens)}</dd></div>
    <div><dt>{t('usage.cacheRead')}</dt><dd>{number(locale, usage?.cacheReadTokens)}</dd></div>
    <div><dt>{t('usage.cacheWrite')}</dt><dd>{number(locale, usage?.cacheWriteTokens)}</dd></div>
    <div><dt>{t('usage.output')}</dt><dd>{number(locale, usage?.outputTokens)}</dd></div>
    <div><dt>{t('usage.reasoning')}</dt><dd>{number(locale, usage?.reasoningTokens)}</dd></div>
    <div><dt>{t('usage.requests')}</dt><dd>{number(locale, usage?.requestCount)}</dd></div>
    <div><dt>{t('usage.speed')}</dt><dd>{usage?.outputTokensPerSecond === undefined ? t('usage.unavailable') : `${number(locale, usage.outputTokensPerSecond)}/s`}</dd></div>
    <div><dt>{t('usage.cost')}</dt><dd>{money(locale, usage?.cost)}</dd></div>
  </dl>
  {#if usage?.cost?.estimated}<p class="usage-note">{t('usage.estimated')}</p>{/if}
</aside>
