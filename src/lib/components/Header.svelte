<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import type { Locale, ProjectSnapshot, ProviderModel, SessionSnapshot, ThinkingEffort } from '../types';
  import { translate } from '../i18n';

  export let locale: Locale;
  export let session: SessionSnapshot | undefined;
  export let models: ProviderModel[] = [];
  export let project: ProjectSnapshot | undefined;
  export let preferredProviderId: string | undefined = undefined;
  export let preferredModelId: string | undefined = undefined;
  export let taskActive = false;
  export let usageActive = false;
  export let taskEnabled = true;
  export let onTasks: () => void;
  export let onUsage: () => void;
  export let onSelectModel: (providerId: string, modelId: string) => Promise<void>;
  export let onConfigureThinking: (providerId: string, modelId: string, enabled: boolean, effort: ThinkingEffort) => Promise<void>;

  let selectingModel = false;
  let modelOpen = false;
  let configuringThinking = false;
  let modelMenu: HTMLDivElement;

  $: t = (key: string) => translate(locale, key);
  $: selected = models.find((model) => model.modelId === session?.modelId && model.providerId === session.providerId)
    ?? models.find((model) => model.modelId === preferredModelId && model.providerId === preferredProviderId)
    ?? models.find((model) => model.available);
  $: availableModels = models.filter((model) => model.available);
  $: selectedKey = selected ? `${selected.providerId}\u001f${selected.modelId}` : '';

  async function chooseModel(providerId: string, modelId: string) {
    if (!providerId || !modelId || selectingModel || session?.status === 'streaming') return;
    modelOpen = false;
    selectingModel = true;
    try { await onSelectModel(providerId, modelId); } finally { selectingModel = false; }
  }

  async function configureThinking(enabled: boolean, effort: ThinkingEffort) {
    if (!selected || configuringThinking || session?.status === 'streaming') return;
    configuringThinking = true;
    try {
      await onConfigureThinking(selected.providerId, selected.modelId, enabled, effort);
    } finally {
      configuringThinking = false;
    }
  }

  function closeModelMenu(event: PointerEvent) {
    if (modelOpen && modelMenu && !modelMenu.contains(event.target as Node)) modelOpen = false;
  }

  function handleEscape(event: KeyboardEvent) {
    if (event.key === 'Escape') modelOpen = false;
  }

  onMount(() => {
    document.addEventListener('pointerdown', closeModelMenu);
    document.addEventListener('keydown', handleEscape);
  });

  onDestroy(() => {
    document.removeEventListener('pointerdown', closeModelMenu);
    document.removeEventListener('keydown', handleEscape);
  });

  async function windowAction(action: 'minimize' | 'maximize' | 'close') {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const window = getCurrentWindow();
    if (action === 'minimize') await window.minimize();
    else if (action === 'maximize') await window.toggleMaximize();
    else await window.close();
  }
</script>

<header class="main-header" data-tauri-drag-region>
  <div class="header-context" data-tauri-drag-region>
    <div class="header-title">{session?.title ? t(session.title) : t('app.newConversation')}</div>
    <div class="header-meta">
      {#if project?.name}<span>{project.name}</span><span class="meta-divider"></span>{/if}
      <span>{t(`sidebar.index.${project?.status ?? 'unavailable'}`)}</span>
    </div>
  </div>
  <div class="header-actions">
    <div class:open={modelOpen} class="model-picker" bind:this={modelMenu}>
      <button
        class="model-trigger"
        type="button"
        disabled={selectingModel || session?.status === 'streaming' || !availableModels.length}
        aria-label={t('settings.models')}
        aria-haspopup="listbox"
        aria-expanded={modelOpen}
        title={t('settings.models')}
        on:click={() => modelOpen = !modelOpen}
      >
        <span class:online={selected?.available} class="model-indicator"></span>
        <strong>{selected?.modelName ?? t(models.length ? 'header.modelUnavailable' : 'header.modelLoading')}</strong>
        <Icon name="chevron" size={13} />
      </button>
      {#if modelOpen}
        <div class="model-popover" aria-label={t('settings.models')}>
          <div class="model-option-list" role="listbox">
            {#each availableModels as model (`${model.providerId}:${model.modelId}`)}
              <button
                class:selected={`${model.providerId}\u001f${model.modelId}` === selectedKey}
                class="model-option"
                type="button"
                role="option"
                aria-selected={`${model.providerId}\u001f${model.modelId}` === selectedKey}
                on:click={() => chooseModel(model.providerId, model.modelId)}
              >
                <span class="model-option-indicator"></span>
                <span><strong>{model.modelName}</strong><small>{model.providerName} · {model.adapter}</small></span>
                {#if `${model.providerId}\u001f${model.modelId}` === selectedKey}<Icon name="check" size={14} />{/if}
              </button>
            {/each}
          </div>
          {#if selected}
            <div class="model-thinking-panel">
              <span class="thinking-panel-title">{t('settings.providers.thinking')}</span>
              {#if selected.thinkingEfforts.length}
                <button
                  class:active={selected.thinkingEnabled}
                  class="thinking-toggle"
                  type="button"
                  disabled={configuringThinking || session?.status === 'streaming'}
                  aria-pressed={selected.thinkingEnabled}
                  on:click={() => configureThinking(!selected.thinkingEnabled, selected.thinkingEffort)}
                ><i></i><span>{t(selected.thinkingEnabled ? 'settings.providers.thinkingOn' : 'settings.providers.thinkingOff')}</span></button>
                <select
                  value={selected.thinkingEffort}
                  disabled={!selected.thinkingEnabled || configuringThinking || session?.status === 'streaming'}
                  aria-label={t('settings.providers.thinkingEffort')}
                  on:change={(event) => configureThinking(true, event.currentTarget.value as ThinkingEffort)}
                >
                  {#each selected.thinkingEfforts as effort}<option value={effort}>{t(`settings.providers.effort.${effort}`)}</option>{/each}
                </select>
              {:else if selected.capabilities?.includes('reasoning')}
                <strong class="thinking-auto">{t('settings.providers.thinkingAutomatic')}</strong>
              {:else}
                <strong class="thinking-auto muted">{t('settings.providers.thinkingUnsupported')}</strong>
              {/if}
            </div>
          {/if}
        </div>
      {/if}
    </div>
    {#if taskEnabled}<button class:active={taskActive} type="button" on:click={onTasks} aria-label={t('app.tasks')} title={t('app.tasks')}><Icon name="tasks" /></button>{/if}
    <button class:active={usageActive} type="button" on:click={onUsage} aria-label={t('usage.title')} title={t('usage.title')}><Icon name="usage" /></button>
    <span class="window-controls" aria-label={t('window.controls')}>
      <button type="button" on:click={() => windowAction('minimize')} aria-label={t('window.minimize')} title={t('window.minimize')}><Icon name="minimize" size={16} /></button>
      <button type="button" on:click={() => windowAction('maximize')} aria-label={t('window.maximize')} title={t('window.maximize')}><Icon name="maximize" size={14} /></button>
      <button class="window-close" type="button" on:click={() => windowAction('close')} aria-label={t('app.close')} title={t('app.close')}><Icon name="close" size={15} /></button>
    </span>
  </div>
</header>
