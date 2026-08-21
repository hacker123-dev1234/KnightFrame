<script lang="ts">
  import Icon from './Icon.svelte';
  import StatusMark from './StatusMark.svelte';
  import type { Locale, TaskSnapshot } from '../types';
  import { translate } from '../i18n';

  export let locale: Locale;
  export let task: TaskSnapshot | undefined;
  export let onClose: () => void;
  export let onCancel: () => Promise<void>;

  $: t = (key: string, args: Record<string, string | number> = {}) => translate(locale, key, args);
  $: percent = task?.total ? Math.min(100, Math.round(task.completed / task.total * 100)) : 0;
</script>

<aside class="inspector task-inspector" aria-label={t('task.title')}>
  <header>
    <div><span class="inspector-eyebrow">{t('task.title')}</span><h2>{task?.current ? t(task.current) : t('task.empty')}</h2></div>
    <button type="button" on:click={onClose} aria-label={t('app.close')}><Icon name="close" /></button>
  </header>
  {#if task}
    <div class="task-progress-copy">
      <span>{t('task.progress', { done: task.completed, total: task.total })}</span>
      <StatusMark status={task.status} label={t(`status.${task.status}`)} />
    </div>
    <div class="progress-track" aria-label={t('task.progress', { done: task.completed, total: task.total })}><span style={`width:${percent}%`}></span></div>
    <ol class="task-list">
      {#each task.items as item (item.id)}
        <li class:current={item.status === 'running'}>
          <span class="task-index"><Icon name={item.status === 'completed' ? 'check' : item.status === 'blocked' ? 'shield' : 'spark'} size={14} /></span>
          <div><strong>{t(item.title)}</strong>{#if item.detail}<p>{t(item.detail)}</p>{/if}</div>
          <StatusMark status={item.status} label={t(`status.${item.status}`)} />
        </li>
      {/each}
    </ol>
    {#if task.status === 'running' || task.status === 'pending'}
      <button class="inspector-action" type="button" on:click={() => onCancel()}><Icon name="stop" size={15} />{t('task.cancel')}</button>
    {/if}
  {/if}
</aside>
