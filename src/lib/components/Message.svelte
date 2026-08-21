<script lang="ts">
  import Icon from './Icon.svelte';
  import Workflow from './Workflow.svelte';
  import AuxiliaryCard from './AuxiliaryCard.svelte';
  import MarketChartCard from './MarketChartCard.svelte';
  import type { Locale, MessageSnapshot } from '../types';
  import { translate } from '../i18n';
  import { renderMarkdown } from '../markdown';

  export let locale: Locale;
  export let message: MessageSnapshot;
  export let streaming = false;
  export let userAvatar: string | undefined = undefined;

  let copied = false;
  let reasoningOpen = false;
  $: t = (key: string) => translate(locale, key);
  // 流式期间只做纯文本追加；markdown 全量解析（marked+DOMPurify+DOMParser）
  // 随内容增长是 O(n²)，每个 delta 重算会冻结 UI，仅在定稿后渲染一次。
  $: renderedContent = message.role === 'assistant' && !streaming ? renderMarkdown(message.content) : '';
  // 行情直出区：market 工具不进工作流折叠 —— K 线图是给人看的。
  $: marketCalls = (message.tools ?? []).filter((tool) => tool.name === 'market');
  $: workflowTools = (message.tools ?? []).filter((tool) => tool.name !== 'market');

  async function copy() {
    await navigator.clipboard.writeText(message.content);
    copied = true;
    setTimeout(() => copied = false, 1200);
  }
</script>

<article class:assistant={message.role === 'assistant'} class="message">
  <div class="message-rail">
    {#if message.role === 'assistant'}
      <span class="assistant-mark"><img src="/brand/knightframe-primary-white.png" alt="" /></span>
    {:else}
      <span class="user-mark">{#if userAvatar}<img src={userAvatar} alt="" />{:else}{locale === 'zh-CN' ? '你' : 'U'}{/if}</span>
    {/if}
  </div>
  <div class="message-body">
    <div class="message-label">{t(message.role === 'assistant' ? 'conversation.assistant' : 'conversation.you')}</div>

    {#if message.auxiliary?.length}
      <div class="auxiliary-stack">
        {#each message.auxiliary as receipt (receipt.id)}<AuxiliaryCard {locale} {receipt} />{/each}
      </div>
    {/if}

    {#if message.reasoning?.length}
      <section class:active={streaming} class="reasoning-block">
        <button type="button" on:click={() => reasoningOpen = !reasoningOpen} aria-expanded={reasoningOpen}>
          <span class="reasoning-sigil"></span>
          <span>{t('conversation.reasoning')}</span>
          {#if streaming}<span class="reasoning-live">{t('conversation.streaming')}</span>{/if}
          <Icon name="chevron" size={14} />
        </button>
        {#if reasoningOpen}
          <div class="reasoning-content">
            {#each message.reasoning as block (block.id)}<p>{block.summary}</p>{/each}
            <small>{t('conversation.reasoning.note')}</small>
          </div>
        {/if}
      </section>
    {/if}

    {#if marketCalls.length}
      <div class="market-direct">
        {#each marketCalls as call (call.id)}
          <MarketChartCard {locale} callId={call.id} status={call.status} />
        {/each}
      </div>
    {/if}

    {#if workflowTools.length}
      <Workflow {locale} tools={workflowTools} />
    {/if}

    {#if message.attachments?.length}
      <div class="message-attachments">{#each message.attachments as attachment (attachment.id)}<figure><img src={attachment.dataUrl} alt={attachment.name} /><figcaption>{attachment.name}</figcaption></figure>{/each}</div>
    {/if}

    {#if message.content}
      {#if message.role === 'assistant' && !streaming && renderedContent}
        <div class="message-content markdown-body">{@html renderedContent}</div>
      {:else}
        <div class:streaming class="message-content">{message.content}</div>
      {/if}
      {#if message.role === 'assistant' && !streaming}
        <button class="copy-action" type="button" on:click={copy} aria-label={t('app.copy')} title={t('app.copy')}>
          <Icon name={copied ? 'check' : 'copy'} size={14} />{copied ? t('app.copied') : t('app.copy')}
        </button>
      {/if}
    {/if}
  </div>
</article>
