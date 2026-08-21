<script lang="ts">
  import Icon from '../components/Icon.svelte';
  import Composer from '../components/Composer.svelte';
  import Message from '../components/Message.svelte';
  import type { Locale, MessageAttachment, SessionSnapshot } from '../types';
  import { translate } from '../i18n';

  export let locale: Locale;
  export let available: boolean;
  export let session: SessionSnapshot | undefined;
  export let userAvatar: string | undefined = undefined;
  export let imageInput = false;
  export let onClose: () => void;
  export let onSend: (content: string, clarify?: boolean, attachments?: MessageAttachment[]) => Promise<void>;
  export let onStop: () => Promise<void>;

  $: t = (key: string) => translate(locale, key);
</script>

<section class="mini-page">
  <header><div class="mini-mark"><Icon name="spark" /></div><div><h1>{t('app.brand')}</h1><span>{t('mini.title')}</span></div><button type="button" on:click={onClose} aria-label={t('app.close')}><Icon name="close" /></button></header>
  <div class="mini-body">
    {#if !available}<div class="mini-unavailable"><Icon name="shield" size={30} /><p>{t('mini.unavailable')}</p></div>
    {:else if session}{#each session.messages as message, index (message.id)}<Message {locale} {message} {userAvatar} streaming={session.status === 'streaming' && index === session.messages.length - 1} />{/each}{/if}
  </div>
  {#if available}<Composer {locale} compact {imageInput} streaming={session?.status === 'streaming'} {onSend} {onStop} />{/if}
</section>
