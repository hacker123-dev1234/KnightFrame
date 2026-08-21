<script lang="ts">
  import Icon from './Icon.svelte';
  import type { Locale, MessageAttachment } from '../types';
  import { translate } from '../i18n';

  export let locale: Locale;
  export let disabled = false;
  export let streaming = false;
  export let compact = false;
  export let initial = '';
  export let imageInput = false;
  export let onSend: (content: string, clarify?: boolean, attachments?: MessageAttachment[]) => Promise<void>;
  export let onStop: () => Promise<void>;

  let content = initial;
  let sending = false;
  let showClarification = false;
  let attachments: MessageAttachment[] = [];
  let fileInput: HTMLInputElement;
  let textarea: HTMLTextAreaElement;
  let dropActive = false;
  let attachmentError = '';

  $: t = (key: string) => translate(locale, key);
  $: if (initial && !content) content = initial;

  async function submit(skipClarification = false, clarify = false) {
    const value = content.trim();
    if ((!value && !attachments.length) || disabled || sending) return;
    if (!skipClarification && [...value].length > 200) {
      showClarification = true;
      return;
    }
    showClarification = false;
    sending = true;
    try {
      await onSend(value, clarify, attachments);
      content = '';
      attachments = [];
      resizeTextarea();
    } finally {
      sending = false;
    }
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      void submit();
    }
  }

  function resizeTextarea() {
    if (!textarea) return;
    textarea.style.height = '0px';
    textarea.style.height = `${Math.min(144, Math.max(24, textarea.scrollHeight))}px`;
  }

  function readDataUrl(file: File): Promise<string> {
    return new Promise((resolve, reject) => { const reader = new FileReader(); reader.onload = () => resolve(String(reader.result)); reader.onerror = () => reject(reader.error); reader.readAsDataURL(file); });
  }

  async function stageFiles(files: FileList | File[]) {
    attachmentError = '';
    if (!imageInput) { attachmentError = t('composer.imageUnsupported'); return; }
    const accepted = ['image/png', 'image/jpeg', 'image/webp', 'image/gif'];
    for (const file of Array.from(files)) {
      if (!accepted.includes(file.type) || file.size > 20 * 1024 * 1024) { attachmentError = t('composer.imageInvalid'); continue; }
      if (attachments.length >= 8) { attachmentError = t('composer.imageLimit'); break; }
      const dataUrl = await readDataUrl(file);
      attachments = [...attachments, { id: crypto.randomUUID(), name: file.name, mimeType: file.type, dataUrl, size: file.size }];
    }
  }

  function dragOver(event: DragEvent) { if (!event.dataTransfer?.types.includes('Files')) return; event.preventDefault(); dropActive = true; }
  function dragLeave(event: DragEvent) { if (!(event.currentTarget as HTMLElement).contains(event.relatedTarget as Node | null)) dropActive = false; }
  function drop(event: DragEvent) { if (!event.dataTransfer?.files.length) return; event.preventDefault(); dropActive = false; void stageFiles(event.dataTransfer.files); }
</script>

<div class:compact class:drop-active={dropActive} class="composer-wrap" role="group" aria-label={t('composer.placeholder')} on:dragover={dragOver} on:dragleave={dragLeave} on:drop={drop}>
  {#if showClarification}
    <div class="clarification" role="dialog" aria-modal="true" aria-labelledby="clarification-title">
      <div>
        <h3 id="clarification-title">{t('clarification.large_input.title')}</h3>
        <p>{t('clarification.large_input.body')}</p>
      </div>
      <div class="clarification-actions">
        <button class="text-button" type="button" on:click={() => showClarification = false}>{t('clarification.large_input.cancel')}</button>
        <button class="text-button" type="button" on:click={() => submit(true)}>{t('clarification.large_input.skip')}</button>
        <button class="primary-button" type="button" on:click={() => submit(true, true)}>{t('clarification.large_input.enable')}</button>
      </div>
    </div>
  {/if}
  {#if attachments.length}<div class="attachment-tray">{#each attachments as attachment (attachment.id)}<div class="attachment-chip"><img src={attachment.dataUrl} alt="" /><span><strong>{attachment.name}</strong><small>{Math.ceil(attachment.size / 1024)} KB</small></span><button type="button" on:click={() => attachments = attachments.filter((item) => item.id !== attachment.id)} aria-label={t('composer.imageRemove')}><Icon name="close" size={12} /></button></div>{/each}</div>{/if}
  {#if attachmentError}<p class="attachment-error">{attachmentError}</p>{/if}
  <div class:focused={content.length > 0 || attachments.length > 0} class:streaming class="composer">
    <input bind:this={fileInput} class="composer-file-input" type="file" multiple accept="image/png,image/jpeg,image/webp,image/gif" on:change={(event) => { void stageFiles(event.currentTarget.files ?? []); event.currentTarget.value = ''; }} />
    <button class="composer-icon" type="button" disabled={!imageInput || disabled} on:click={() => fileInput.click()} aria-label={imageInput ? t('composer.attach') : t('composer.imageUnsupported')} title={imageInput ? t('composer.attach') : t('composer.imageUnsupported')}><Icon name="paperclip" /></button>
    <textarea
      bind:this={textarea}
      bind:value={content}
      on:keydown={keydown}
      on:input={resizeTextarea}
      rows="1"
      placeholder={streaming ? t('composer.guidePlaceholder') : disabled ? t('composer.modelMissing') : t('composer.placeholder')}
      aria-label={t('composer.placeholder')}
      {disabled}
    ></textarea>
    {#if streaming}
      <span class="composer-stream-actions">
        <button class="send-button guide" type="button" disabled={(!content.trim() && !attachments.length) || sending} on:click={() => submit()} aria-label={t('composer.guide')} title={t('composer.guide')}><Icon name="send" size={16} /></button>
        <button class="send-button stop" type="button" on:click={() => onStop()} aria-label={t('app.stop')} title={t('app.stop')}><Icon name="stop" size={15} /></button>
      </span>
    {:else}
      <button class="send-button" type="button" disabled={disabled || (!content.trim() && !attachments.length) || sending} on:click={() => submit()} aria-label={t('composer.send')} title={t('composer.send')}><Icon name="send" size={17} /></button>
    {/if}
  </div>
</div>
