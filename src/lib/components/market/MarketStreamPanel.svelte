<script lang="ts">
  import { tick } from 'svelte';
  import Icon from '../Icon.svelte';
  import { translate } from '../../i18n';
  import type { Locale, MarketChatMessage } from '../../types';
  import type { MarketStageState } from '../../market-state';

  export let locale: Locale;
  export let stage1: MarketStageState;
  export let stage2: MarketStageState;
  export let gateWait: boolean;
  export let statusMessage: string | undefined;
  export let statusError: boolean;
  export let chat: MarketChatMessage[];
  export let chatDraft: MarketChatMessage | undefined;
  export let chatStreaming: boolean;
  export let canChat: boolean;
  export let onSendChat: (text: string) => void;
  export let onStopChat: () => void;

  let draft = '';
  let streamBody: HTMLElement;
  let chatBody: HTMLElement;

  $: t = (key: string, args: Record<string, string | number> = {}) => translate(locale, key, args);
  $: stageLabel = (stage: 'stage1' | 'stage2') => t(`market.stage.${stage}`);
  $: hasStreamOutput = stage1.reasoning || stage1.content || stage2.reasoning || stage2.content;
  $: chatView = chatDraft ? [...chat, chatDraft] : chat;

  $: if (stage1.reasoning || stage1.content || stage2.reasoning || stage2.content) {
    void tick().then(() => streamBody?.scrollTo({ top: streamBody.scrollHeight }));
  }
  $: if (chatDraft || chat.length) {
    void tick().then(() => chatBody?.scrollTo({ top: chatBody.scrollHeight }));
  }

  function submit() {
    const text = draft.trim();
    if (!text || chatStreaming) return;
    draft = '';
    onSendChat(text);
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      submit();
    }
  }
</script>

<div class="market-stream">
  <div class="stream-scroll" bind:this={streamBody}>
    {#if !hasStreamOutput && !statusMessage}
      <div class="stream-empty">
        <Icon name="market" size={26} />
        <p>{t('market.stream.empty')}</p>
      </div>
    {/if}

    {#if statusMessage}
      <div class="stream-status" class:error={statusError}>
        {#if statusError}<Icon name="alert" size={14} />{:else}<span class="pulse"></span>{/if}
        <span>{statusMessage}</span>
      </div>
    {/if}

    {#if gateWait}
      <div class="stream-status gate">
        <Icon name="clock" size={14} />
        <span>{t('market.gateWait')}</span>
      </div>
    {/if}

    {#each ['stage1', 'stage2'] as stageKey (stageKey)}
      {@const stage = stageKey === 'stage1' ? stage1 : stage2}
      {#if stage.status !== 'idle'}
        <section class="stage-block">
          <header>
            <strong>{stageLabel(stageKey as 'stage1' | 'stage2')}</strong>
            <span class="stage-state {stage.status}">
              {stage.status === 'running' ? t('status.running') : stage.status === 'retry' ? t('status.running') : stage.status === 'failed' ? t('status.failed') : t('status.completed')}
            </span>
          </header>
          {#if stage.retries.length}
            <ul class="retry-list">
              {#each stage.retries as retry, index (index)}
                <li>{t('market.stage.retry', { attempt: retry.attempt || index + 1, message: retry.message })}</li>
              {/each}
            </ul>
          {/if}
          {#if stage.reasoning}
            <details class="reasoning" open={stage.status === 'running' && !stage.content}>
              <summary>{t('market.stream.reasoning')}</summary>
              <pre>{stage.reasoning}</pre>
            </details>
          {/if}
          {#if stage.content}
            <pre class="stage-content">{stage.content}</pre>
          {:else if stage.status === 'running' && !stage.reasoning}
            <p class="waiting">{t('market.stream.waiting')}</p>
          {/if}
        </section>
      {/if}
    {/each}
  </div>

  <div class="chat-region">
    <div class="chat-scroll" bind:this={chatBody}>
      {#if !chatView.length}
        <p class="chat-empty">{t('market.chat.empty')}</p>
      {/if}
      {#each chatView as message (message.id)}
        <article class="chat-message {message.role}">
          <span class="chat-label">{message.role === 'user' ? t('market.chat.you') : t('market.chat.assistant')}</span>
          {#if message.reasoning}
            <details class="reasoning small">
              <summary>{t('market.stream.reasoning')}</summary>
              <pre>{message.reasoning}</pre>
            </details>
          {/if}
          <pre class="chat-content">{message.content}</pre>
        </article>
      {/each}
    </div>
    <div class="chat-input">
      <textarea
        rows="1"
        bind:value={draft}
        on:keydown={keydown}
        placeholder={canChat ? t('market.chat.placeholder') : t('market.chat.empty')}
        disabled={!canChat || chatStreaming}
      ></textarea>
      {#if chatStreaming}
        <button type="button" class="chat-stop" on:click={onStopChat} aria-label={t('market.chat.stop')} title={t('market.chat.stop')}>
          <Icon name="stop" size={15} />
        </button>
      {:else}
        <button type="button" class="chat-send" disabled={!canChat || !draft.trim()} on:click={submit} aria-label={t('market.chat.send')} title={t('market.chat.send')}>
          <Icon name="send" size={15} />
        </button>
      {/if}
    </div>
  </div>
</div>

<style>
  .market-stream {
    display: grid;
    grid-template-rows: minmax(0, 1.35fr) minmax(0, 1fr);
    gap: 12px;
    width: 100%;
    height: 100%;
    min-height: 0;
  }
  .stream-scroll,
  .chat-scroll {
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: #2d2d2d transparent;
  }
  .stream-empty {
    display: grid;
    place-items: center;
    gap: 9px;
    height: 100%;
    color: var(--dim);
    font-size: 12px;
    text-align: center;
  }
  .stream-empty p { margin: 0; }
  .stream-status {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-bottom: 10px;
    padding: 9px 12px;
    border: 1px solid var(--border-soft);
    border-radius: var(--radius-sm);
    color: var(--muted);
    background: var(--surface-1);
    font-size: 12px;
  }
  .stream-status.error { color: #e3b9b9; border-color: #33211c; background: #161010; }
  .stream-status.gate { color: var(--text-soft); }
  .pulse {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text);
    box-shadow: 0 0 8px rgba(255, 255, 255, 0.7);
    animation: chatPulse 1.8s ease-in-out infinite;
  }
  @keyframes chatPulse {
    0%, 100% { opacity: 0.35; }
    50% { opacity: 1; }
  }
  .stage-block {
    margin-bottom: 14px;
    padding: 12px;
    border: 1px solid var(--border-soft);
    border-radius: var(--radius-md);
    background: var(--surface-1);
  }
  .stage-block header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 8px;
  }
  .stage-block header strong {
    color: var(--text);
    font: 600 12px var(--serif);
    letter-spacing: 0.02em;
  }
  .stage-state {
    color: var(--dim);
    font: 500 10px var(--mono);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .stage-state.running { color: var(--text-soft); }
  .stage-state.done { color: #9fb8a4; }
  .stage-state.failed { color: #d99c9c; }
  .retry-list {
    margin: 0 0 8px;
    padding: 0 0 0 4px;
    color: #b3936d;
    font-size: 11px;
    list-style: none;
  }
  .retry-list li::before { content: '↻ '; }
  details.reasoning {
    margin-bottom: 8px;
    border: 1px dashed var(--border-soft);
    border-radius: var(--radius-sm);
  }
  details.reasoning summary {
    padding: 7px 10px;
    color: var(--dim);
    cursor: pointer;
    font: 500 10px var(--serif);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  details.reasoning pre {
    max-height: 220px;
    margin: 0;
    overflow-y: auto;
    padding: 0 12px 10px;
    color: #8a8a8a;
    font: 11px/1.65 var(--mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  details.reasoning.small pre { max-height: 140px; font-size: 10px; }
  .stage-content, .chat-content {
    margin: 0;
    color: var(--text-soft);
    font: 12px/1.75 var(--serif);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .waiting { margin: 4px 0 0; color: var(--dim); font-size: 12px; }
  .chat-region {
    display: grid;
    grid-template-rows: minmax(0, 1fr) auto;
    gap: 8px;
    min-height: 0;
    padding-top: 10px;
    border-top: 1px solid var(--border-soft);
  }
  .chat-empty { margin: 8px 2px; color: var(--dim); font-size: 12px; }
  .chat-message { margin-bottom: 12px; }
  .chat-message.user { text-align: right; }
  .chat-label {
    display: block;
    margin-bottom: 4px;
    color: var(--dim);
    font: 600 9px var(--mono);
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }
  .chat-message .chat-content {
    display: inline-block;
    padding: 9px 12px;
    border: 1px solid var(--border-soft);
    border-radius: 13px;
    background: var(--surface-1);
    text-align: left;
  }
  .chat-message.user .chat-content {
    color: var(--inverse);
    border-color: var(--border);
    background: var(--text);
  }
  .chat-input {
    position: relative;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 34px;
    align-items: end;
    gap: 6px;
  }
  .chat-input textarea {
    min-height: 38px;
    max-height: 110px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 13px;
    outline: none;
    resize: none;
    color: var(--text);
    background: #101010;
    font: 12px/1.55 var(--serif);
    transition: border-color 0.4s var(--ease-soft), box-shadow 0.6s var(--ease-soft);
  }
  .chat-input textarea:focus-within,
  .chat-input textarea:focus {
    border-color: rgba(255, 255, 255, 0.24);
    box-shadow: 0 0 22px rgba(255, 255, 255, 0.05);
  }
  .chat-send, .chat-stop {
    display: grid;
    place-items: center;
    width: 34px;
    height: 38px;
    border-radius: 12px;
    cursor: pointer;
    transition: transform 0.4s var(--ease-out), box-shadow 0.6s var(--ease-soft), background-color 0.4s var(--ease-soft);
  }
  .chat-send {
    color: var(--inverse);
    background: #eee;
  }
  .chat-send:hover:not(:disabled) { transform: translateY(-1px); background: #fff; box-shadow: 0 0 22px rgba(255, 255, 255, 0.18); }
  .chat-stop {
    color: var(--text);
    border: 1px solid var(--border-strong);
    background: #1b1b1b;
  }
</style>
