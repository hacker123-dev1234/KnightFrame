<script lang="ts">
  import Composer from '../components/Composer.svelte';
  import type { Locale, MessageAttachment } from '../types';
  import { translate } from '../i18n';

  export let locale: Locale;
  export let canSend: boolean;
  export let imageInput = false;
  export let onSend: (content: string, clarify?: boolean, attachments?: MessageAttachment[]) => Promise<void>;
  export let onStop: () => Promise<void>;

  let suggestion = '';
  $: t = (key: string) => translate(locale, key);
  const suggestions = ['empty.suggestion.explain', 'empty.suggestion.fix', 'empty.suggestion.plan'];
</script>

<section class="empty-page">
  <div class="empty-ambient"></div>
  <div class="empty-content">
    <p class="empty-eyebrow">{t('empty.eyebrow')}</p>
    <h1>{t('empty.title')}</h1>
    <p class="empty-subtitle">{t('empty.subtitle')}</p>
    <div class="gate-crop" aria-hidden="true"><span class="gate-aura"></span><img src="/brand/knightframe-sword-gate-ui.png" alt="" /></div>
    <Composer {locale} disabled={!canSend} {imageInput} initial={suggestion} {onSend} {onStop} />
    <div class="suggestions">
      {#each suggestions as key}<button type="button" on:click={() => suggestion = t(key)}>{t(key)}</button>{/each}
    </div>
    <small class="empty-disclaimer">{t('empty.disclaimer')}</small>
  </div>
</section>
