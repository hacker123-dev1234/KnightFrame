<script lang="ts">
  import Icon from '../components/Icon.svelte';
  import type { ConfiguredModel, Locale, ProviderAdapter, ProviderModel, ProviderProfile, ProviderTemplate, SettingsSnapshot, ThinkingEffort } from '../types';
  import { bridge } from '../bridge';
  import { translate } from '../i18n';
  import { applyUiScale } from '../uiScale';

  export let settings: SettingsSnapshot;
  export let providers: ProviderModel[];
  export let providerTemplates: ProviderTemplate[] = [];
  export let modelSelectionDisabled = false;
  export let onBack: () => void;
  export let onUpdate: (patch: Partial<SettingsSnapshot>) => Promise<void>;
  export let onSelectModel: (providerId: string, modelId: string) => Promise<void>;

  let saving = false;
  $: locale = settings.locale;
  $: t = (key: string, args: Record<string, string | number> = {}) => translate(locale, key, args);
  $: selectedProviderId = settings.providerId ?? providers.find((model) => model.available)?.providerId;
  $: selectedModelId = settings.modelId ?? providers.find((model) => model.available)?.modelId;
  $: auxiliaryProviderId = settings.auxiliaryProviderId ?? '';
  $: auxiliaryModelId = settings.auxiliaryModelId ?? '';
  $: subagentExecutionKey = settings.subagentExecutionProviderId && settings.subagentExecutionModelId
    ? `${settings.subagentExecutionProviderId}::${settings.subagentExecutionModelId}` : '';
  let profiles: ProviderProfile[] = structuredClone(settings.providers ?? []);
  let templateId = 'openai';
  let probing = '';
  let providerNotice = '';
  let auxiliarySource: 'local' | 'network' = 'local';
  let auxiliaryName = '';
  let auxiliaryAdapter: ProviderAdapter = 'openai';
  let auxiliaryBaseUrl = 'http://127.0.0.1:11434/v1';
  let auxiliaryUserAgent = '';
  let auxiliaryApiKey = '';
  let auxiliaryNewModelId = '';
  let auxiliaryDiscovered: ConfiguredModel[] = [];
  let auxiliaryNotice = '';
  let auxiliaryProbing = false;
  let lastProfileSignature = JSON.stringify(settings.providers ?? []);
  $: if (JSON.stringify(settings.providers ?? []) !== lastProfileSignature) {
    profiles = structuredClone(settings.providers ?? []);
    lastProfileSignature = JSON.stringify(settings.providers ?? []);
  }

  function uniqueProviderId(seed: string): string {
    const base = seed.replace(/[^a-zA-Z0-9_-]/g, '-').toLowerCase() || 'provider';
    let id = base; let index = 2;
    while (profiles.some((profile) => profile.id === id)) id = `${base}-${index++}`;
    return id;
  }

  function addProvider() {
    const template = providerTemplates.find((item) => item.id === templateId) ?? providerTemplates[0];
    if (!template) return;
    profiles = [...profiles, { id: uniqueProviderId(template.id), name: template.name, adapter: template.adapter, baseUrl: template.baseUrl, userAgent: '', apiKey: '', credentialRef: '', models: [] }];
  }

  function patchProvider(id: string, patch: Partial<ProviderProfile>) {
    profiles = profiles.map((profile) => profile.id === id ? { ...profile, ...patch } : profile);
  }

  function patchModel(providerId: string, modelId: string, patch: Partial<ConfiguredModel>) {
    profiles = profiles.map((profile) => profile.id === providerId ? { ...profile, models: profile.models.map((model) => model.id === modelId ? { ...model, ...patch } : model) } : profile);
  }

  function addModel(providerId: string) {
    const profile = profiles.find((item) => item.id === providerId); if (!profile) return;
    let id = 'model-id'; let index = 2; while (profile.models.some((model) => model.id === id)) id = `model-id-${index++}`;
    patchProvider(providerId, { models: [...profile.models, { id, name: id, adapter: profile.adapter, capabilities: ['streaming', 'toolCalls'], thinkingEnabled: false, thinkingEffort: 'medium', thinkingToggle: true, thinkingEfforts: ['minimal', 'low', 'medium', 'high'] }] });
  }

  function removeModel(providerId: string, modelId: string) {
    const profile = profiles.find((item) => item.id === providerId); if (!profile) return;
    patchProvider(providerId, { models: profile.models.filter((model) => model.id !== modelId) });
  }

  async function probe(profile: ProviderProfile) {
    probing = profile.id; providerNotice = '';
    try {
      const discovered = await bridge.probeProvider(profile);
      const manual = new Map(profile.models.map((model) => [model.id, model]));
      patchProvider(profile.id, { models: discovered.map((model) => ({
        ...model,
        capabilities: manual.get(model.id)?.capabilities ?? model.capabilities,
        adapter: manual.get(model.id)?.adapter ?? model.adapter,
        contextWindow: manual.get(model.id)?.contextWindow ?? model.contextWindow,
        thinkingEnabled: manual.get(model.id)?.thinkingEnabled ?? model.thinkingEnabled ?? false,
        thinkingEffort: manual.get(model.id)?.thinkingEffort ?? model.thinkingEffort ?? 'medium',
        thinkingToggle: model.thinkingToggle ?? manual.get(model.id)?.thinkingToggle ?? false,
        thinkingEfforts: model.thinkingEfforts ?? manual.get(model.id)?.thinkingEfforts ?? [],
      })) });
      providerNotice = t('settings.providers.probeSuccess', { count: discovered.length });
    } catch { providerNotice = t('settings.providers.probeFailed'); }
    finally { probing = ''; }
  }

  async function saveProviders() {
    const first = profiles.flatMap((profile) => profile.models.map((model) => ({ providerId: profile.id, modelId: model.id })))[0];
    const selectedExists = profiles.some((profile) => profile.id === selectedProviderId && profile.models.some((model) => model.id === selectedModelId));
    const auxiliaryExists = profiles.some((profile) => profile.id === auxiliaryProviderId && profile.models.some((model) => model.id === auxiliaryModelId));
    const subagentExists = profiles.some((profile) => profile.id === settings.subagentExecutionProviderId && profile.models.some((model) => model.id === settings.subagentExecutionModelId));
    await update({ providers: profiles, providerId: selectedExists ? selectedProviderId : first?.providerId ?? '', modelId: selectedExists ? selectedModelId : first?.modelId ?? '', auxiliaryEnabled: settings.auxiliaryEnabled && Boolean(first), auxiliaryProviderId: auxiliaryExists ? auxiliaryProviderId : first?.providerId ?? '', auxiliaryModelId: auxiliaryExists ? auxiliaryModelId : first?.modelId ?? '', subagentExecutionProviderId: subagentExists ? settings.subagentExecutionProviderId : '', subagentExecutionModelId: subagentExists ? settings.subagentExecutionModelId : '' });
    providerNotice = t('settings.providers.saved');
  }

  async function update(patch: Partial<SettingsSnapshot>) {
    saving = true;
    try { await onUpdate(patch); } finally { saving = false; }
  }

  async function selectModel(providerId: string, modelId: string) {
    saving = true;
    try { await onSelectModel(providerId, modelId); } finally { saving = false; }
  }

  async function selectAuxiliaryModel(providerId: string, modelId: string) {
    await update({ auxiliaryProviderId: providerId, auxiliaryModelId: modelId });
  }

  async function selectSubagentExecutionModel(value: string) {
    const [providerId = '', modelId = ''] = value.split('::', 2);
    await update({ subagentExecutionProviderId: providerId, subagentExecutionModelId: modelId });
  }

  function chooseAuxiliarySource(source: 'local' | 'network') {
    auxiliarySource = source;
    auxiliaryName = '';
    auxiliaryBaseUrl = source === 'local' ? 'http://127.0.0.1:11434/v1' : '';
    auxiliaryUserAgent = '';
    auxiliaryApiKey = '';
    auxiliaryNewModelId = '';
    auxiliaryDiscovered = [];
    auxiliaryNotice = '';
  }

  function pendingAuxiliaryProfile(): ProviderProfile | undefined {
    const baseUrl = auxiliaryBaseUrl.trim();
    if (!baseUrl) return undefined;
    return {
      id: uniqueProviderId(auxiliarySource === 'local' ? 'local-small-model' : 'remote-small-model'),
      name: auxiliaryName.trim() || t(auxiliarySource === 'local' ? 'settings.auxiliary.localDefaultName' : 'settings.auxiliary.networkDefaultName'),
      adapter: auxiliaryAdapter,
      baseUrl,
      userAgent: auxiliaryUserAgent.trim(),
      apiKey: auxiliarySource === 'network' ? auxiliaryApiKey : '',
      credentialRef: '',
      models: [],
    };
  }

  async function probeAuxiliary() {
    const profile = pendingAuxiliaryProfile();
    if (!profile) { auxiliaryNotice = t('settings.auxiliary.incomplete'); return; }
    auxiliaryProbing = true;
    auxiliaryNotice = '';
    try {
      auxiliaryDiscovered = await bridge.probeProvider(profile);
      if (!auxiliaryNewModelId && auxiliaryDiscovered[0]) auxiliaryNewModelId = auxiliaryDiscovered[0].id;
      auxiliaryNotice = t('settings.providers.probeSuccess', { count: auxiliaryDiscovered.length });
    } catch {
      auxiliaryDiscovered = [];
      auxiliaryNotice = t('settings.providers.probeFailed');
    } finally {
      auxiliaryProbing = false;
    }
  }

  async function saveAuxiliaryEndpoint() {
    const profile = pendingAuxiliaryProfile();
    const modelId = auxiliaryNewModelId.trim();
    if (!profile || !modelId) { auxiliaryNotice = t('settings.auxiliary.incomplete'); return; }
    const discovered = auxiliaryDiscovered.find((model) => model.id === modelId);
    profile.models = [{
      id: modelId,
      name: discovered?.name || modelId,
      capabilities: discovered?.capabilities ?? ['streaming'],
      contextLimit: discovered?.contextLimit,
      thinkingEnabled: false,
      thinkingEffort: 'medium',
      thinkingToggle: discovered?.thinkingToggle ?? false,
      thinkingEfforts: discovered?.thinkingEfforts ?? [],
    }];
    const nextProfiles = [...profiles, profile];
    saving = true;
    try {
      await onUpdate({
        providers: nextProfiles,
        auxiliaryEnabled: true,
        auxiliaryProviderId: profile.id,
        auxiliaryModelId: modelId,
      });
      auxiliaryNotice = t('settings.auxiliary.saved');
    } finally {
      saving = false;
    }
  }

  // 缩放滑条：拖动即时预览（原生 webview zoom，与 App.svelte 同一通路），松手才落库
  function previewScale(value: number) {
    if (Number.isFinite(value)) void applyUiScale(value);
  }
  // 轨道填充比例（0.85–1.30 → 0–100%）
  $: scaleFill = Math.round(((settings.uiScale ?? 1) - 0.85) / (1.30 - 0.85) * 100);

  async function chooseAvatar(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file || !file.type.startsWith('image/')) return;
    const bitmap = await createImageBitmap(file);
    const side = Math.min(bitmap.width, bitmap.height);
    const sourceX = Math.floor((bitmap.width - side) / 2);
    const sourceY = Math.floor((bitmap.height - side) / 2);
    const canvas = document.createElement('canvas');
    canvas.width = 192;
    canvas.height = 192;
    const context = canvas.getContext('2d');
    if (!context) { bitmap.close(); return; }
    context.imageSmoothingEnabled = true;
    context.imageSmoothingQuality = 'high';
    context.drawImage(bitmap, sourceX, sourceY, side, side, 0, 0, 192, 192);
    bitmap.close();
    await update({ userAvatar: canvas.toDataURL('image/webp', 0.88) });
  }
</script>

<section class="settings-page">
  <div class="page-container settings-container">
    <button class="back-link" type="button" on:click={onBack}><Icon name="back" size={15} />{t('app.back')}</button>
    <div class="page-intro" data-tauri-drag-region><span>{t('app.brand')}</span><h1>{t('settings.title')}</h1><p>{t('settings.subtitle')}</p></div>

    <details class="settings-section" open>
      <summary class="settings-section-summary"><div class="settings-section-title"><span>01</span><h2>{t('settings.models')}</h2></div><Icon name="chevron" size={14} /></summary>
      <div class="settings-section-body">
      <p class="settings-section-detail">{t(modelSelectionDisabled ? 'settings.models.busy' : 'settings.models.detail')}</p>
      <div class="provider-add-row">
        <select bind:value={templateId} aria-label={t('settings.providers.template')}>{#each providerTemplates as template}<option value={template.id}>{template.name} · {template.adapter}</option>{/each}</select>
        <button class="text-button" type="button" on:click={addProvider}><Icon name="plus" size={14} />{t('settings.providers.add')}</button>
        <button class="primary-button" type="button" disabled={saving} on:click={saveProviders}>{t('settings.providers.save')}</button>
      </div>
      {#if providerNotice}<p class="provider-notice">{providerNotice}</p>{/if}
      <div class="provider-profiles">
        {#each profiles as profile (profile.id)}
          <details class="provider-profile">
            <summary><span class="model-status online"></span><strong>{profile.name}</strong><code>{profile.adapter}</code><small>{profile.models.length} {t('settings.providers.models')}</small></summary>
            <div class="provider-form">
              <label><span>{t('settings.providers.name')}</span><input value={profile.name} on:change={(event) => patchProvider(profile.id, { name: event.currentTarget.value })} /></label>
              <label><span>{t('settings.providers.id')}</span><input value={profile.id} disabled /></label>
              <label><span>{t('settings.providers.adapter')}</span><select value={profile.adapter} on:change={(event) => patchProvider(profile.id, { adapter: event.currentTarget.value as ProviderAdapter })}><option value="openai-responses">OpenAI Responses</option><option value="openai">OpenAI Chat-compatible</option><option value="anthropic">Anthropic Messages</option><option value="gemini">Gemini generateContent</option></select></label>
              <label class="wide"><span>{t('settings.providers.baseUrl')}</span><input value={profile.baseUrl} on:change={(event) => patchProvider(profile.id, { baseUrl: event.currentTarget.value.trim() })} /></label>
              <label class="wide"><span>{t('settings.providers.userAgent')}</span><input maxlength="256" autocomplete="off" spellcheck="false" value={profile.userAgent ?? ''} placeholder={t('settings.providers.userAgentPlaceholder')} on:change={(event) => patchProvider(profile.id, { userAgent: event.currentTarget.value.trim() })} /><small>{t('settings.providers.userAgentDetail')}</small></label>
              <label class="wide"><span>{t('settings.providers.apiKey')}</span><input type="password" autocomplete="off" value={profile.apiKey} placeholder={profile.credentialRef ? t('settings.providers.apiKeySaved') : t('settings.providers.apiKeyPlaceholder')} on:change={(event) => patchProvider(profile.id, { apiKey: event.currentTarget.value })} /></label>
            </div>
            <div class="provider-actions"><button type="button" disabled={probing === profile.id} on:click={() => probe(profile)}>{probing === profile.id ? t('settings.providers.probing') : t('settings.providers.probe')}</button><button type="button" on:click={() => addModel(profile.id)}>{t('settings.providers.modelAdd')}</button><button class="danger" type="button" on:click={() => profiles = profiles.filter((item) => item.id !== profile.id)}>{t('settings.providers.remove')}</button></div>
            <div class="configured-models">
              {#each profile.models as model (model.id)}
                <div class="configured-model">
                  <input class="model-name-input" value={model.name} aria-label={t('settings.providers.modelName')} on:change={(event) => patchModel(profile.id, model.id, { name: event.currentTarget.value })} />
                  <input value={model.id} aria-label={t('settings.providers.modelId')} on:change={(event) => patchModel(profile.id, model.id, { id: event.currentTarget.value.trim() })} />
                  <select class="model-adapter" value={model.adapter ?? ''} aria-label={t('settings.providers.modelAdapter')} on:change={(event) => patchModel(profile.id, model.id, { adapter: (event.currentTarget.value || undefined) as ProviderAdapter | undefined })}>
                    <option value="">{t('settings.providers.followProvider')}</option><option value="openai">Chat Completions</option><option value="openai-responses">Responses</option><option value="anthropic">Messages</option><option value="gemini">Generate Content</option>
                  </select>
                  <label class="model-option-check"><input type="checkbox" checked={model.capabilities.includes('imageInput')} on:change={(event) => patchModel(profile.id, model.id, { capabilities: event.currentTarget.checked ? [...new Set([...model.capabilities, 'imageInput'])] : model.capabilities.filter((item) => item !== 'imageInput') })} /><span>{t('settings.providers.imageInput')}</span></label>
                  {#if model.thinkingEfforts?.length}
                    <label class="model-option-check"><input type="checkbox" checked={model.thinkingEnabled ?? false} on:change={(event) => patchModel(profile.id, model.id, { thinkingEnabled: event.currentTarget.checked })} /><span>{t('settings.providers.thinking')}</span></label>
                    <select class="thinking-effort" value={model.thinkingEffort ?? model.thinkingEfforts[0]} disabled={!model.thinkingEnabled} aria-label={t('settings.providers.thinkingEffort')} on:change={(event) => patchModel(profile.id, model.id, { thinkingEffort: event.currentTarget.value as ThinkingEffort })}>
                      {#each model.thinkingEfforts as effort}<option value={effort}>{t(`settings.providers.effort.${effort}`)}</option>{/each}
                    </select>
                  {:else if model.capabilities.includes('reasoning')}
                    <span class="thinking-mode-status">{t('settings.providers.thinkingAutomatic')}</span><span></span>
                  {:else}
                    <span class="thinking-mode-status muted">{t('settings.providers.thinkingUnsupported')}</span><span></span>
                  {/if}
                  <button type="button" on:click={() => removeModel(profile.id, model.id)} aria-label={t('settings.providers.modelRemove')}><Icon name="close" size={13} /></button>
                </div>
              {/each}
            </div>
          </details>
        {/each}
      </div>
      {#if providers.length}
        <div class="model-grid">
          {#each providers as model (`${model.providerId}:${model.modelId}`)}
            <button
              class:unavailable={!model.available}
              class:selected={model.providerId === selectedProviderId && model.modelId === selectedModelId}
              class="model-card"
              type="button"
              disabled={saving || modelSelectionDisabled || !model.available}
              aria-pressed={model.providerId === selectedProviderId && model.modelId === selectedModelId}
              aria-label={t('settings.models.select', { model: model.modelName })}
              on:click={() => selectModel(model.providerId, model.modelId)}
            >
              <div class="model-card-top">
                <span class:online={model.available} class="model-status"></span>
                <small>{model.providerName}</small>
                {#if !model.available}<span class="model-choice">{t('settings.models.unavailable')}</span>{/if}
              </div>
              <h3>{model.modelName}</h3>
              <code>{model.modelId}</code>
              {#if model.capabilities?.length}<div class="capability-list">{#each model.capabilities as capability}<span>{capability}</span>{/each}</div>{/if}
            </button>
          {/each}
        </div>
      {:else}<div class="settings-empty">{t('settings.models.empty')}</div>{/if}
      </div>
    </details>

    <details class="settings-section">
      <summary class="settings-section-summary"><div class="settings-section-title"><span>02</span><h2>{t('settings.auxiliary')}</h2></div><Icon name="chevron" size={14} /></summary>
      <div class="settings-section-body">
      <p class="settings-section-detail">{t('settings.auxiliary.detail')}</p>
      <div class="auxiliary-source-picker" role="group" aria-label={t('settings.auxiliary.source')}>
        <button class:active={auxiliarySource === 'local'} type="button" on:click={() => chooseAuxiliarySource('local')}>
          <Icon name="desktop" size={17} />
          <span><strong>{t('settings.auxiliary.local')}</strong><small>{t('settings.auxiliary.local.detail')}</small></span>
        </button>
        <button class:active={auxiliarySource === 'network'} type="button" on:click={() => chooseAuxiliarySource('network')}>
          <Icon name="cloud" size={17} />
          <span><strong>{t('settings.auxiliary.network')}</strong><small>{t('settings.auxiliary.network.detail')}</small></span>
        </button>
      </div>
      <div class="auxiliary-endpoint-form">
        <label><span>{t('settings.providers.name')}</span><input bind:value={auxiliaryName} placeholder={t(auxiliarySource === 'local' ? 'settings.auxiliary.localDefaultName' : 'settings.auxiliary.networkDefaultName')} /></label>
        <label><span>{t('settings.providers.adapter')}</span><select bind:value={auxiliaryAdapter}><option value="openai">Chat Completions API</option><option value="openai-responses">Responses API</option><option value="anthropic">Messages API</option><option value="gemini">Generate Content API</option></select></label>
        <label class="wide"><span>{t('settings.providers.baseUrl')}</span><input bind:value={auxiliaryBaseUrl} placeholder={auxiliarySource === 'local' ? 'http://127.0.0.1:11434/v1' : 'https://api.example.com/v1'} /></label>
        <label class="wide"><span>{t('settings.providers.userAgent')}</span><input maxlength="256" autocomplete="off" spellcheck="false" bind:value={auxiliaryUserAgent} placeholder={t('settings.providers.userAgentPlaceholder')} /></label>
        {#if auxiliarySource === 'network'}<label class="wide"><span>{t('settings.providers.apiKey')}</span><input type="password" autocomplete="off" bind:value={auxiliaryApiKey} placeholder={t('settings.providers.apiKeyPlaceholder')} /></label>{/if}
        <label class="wide"><span>{t('settings.providers.modelId')}</span>
          {#if auxiliaryDiscovered.length}
            <select bind:value={auxiliaryNewModelId}>{#each auxiliaryDiscovered as model (model.id)}<option value={model.id}>{model.name || model.id} · {model.id}</option>{/each}</select>
          {:else}<input bind:value={auxiliaryNewModelId} placeholder={t('settings.auxiliary.modelPlaceholder')} />{/if}
        </label>
        <div class="auxiliary-endpoint-actions wide">
          <button type="button" disabled={saving || auxiliaryProbing} on:click={probeAuxiliary}>{auxiliaryProbing ? t('settings.providers.probing') : t('settings.providers.probe')}</button>
          <button class="primary-button" type="button" disabled={saving || auxiliaryProbing} on:click={saveAuxiliaryEndpoint}>{t('settings.auxiliary.saveUse')}</button>
        </div>
      </div>
      {#if auxiliaryNotice}<p class="provider-notice auxiliary-notice">{auxiliaryNotice}</p>{/if}
      <div class="settings-list">
        <label class="setting-row">
          <div><strong>{t('settings.auxiliary.reducer')}</strong><p>{t('settings.auxiliary.reducer.detail')}</p></div>
          <input type="checkbox" checked={settings.auxiliaryEnabled} disabled={saving} on:change={(event) => update({ auxiliaryEnabled: event.currentTarget.checked })} />
          <span class="switch" aria-hidden="true"></span>
        </label>
      </div>
      {#if providers.length}
        <div class="model-grid auxiliary-model-grid">
          {#each providers as model (`aux:${model.providerId}:${model.modelId}`)}
            <button
              class:unavailable={!model.available}
              class:selected={model.providerId === auxiliaryProviderId && model.modelId === auxiliaryModelId}
              class="model-card compact"
              type="button"
              disabled={saving || !model.available}
              aria-pressed={model.providerId === auxiliaryProviderId && model.modelId === auxiliaryModelId}
              aria-label={t('settings.auxiliary.select', { model: model.modelName })}
              on:click={() => selectAuxiliaryModel(model.providerId, model.modelId)}
            >
              <div class="model-card-top"><span class:online={model.available} class="model-status"></span><small>{model.providerName}</small></div>
              <h3>{model.modelName}</h3>
              <code>{model.modelId}</code>
            </button>
          {/each}
        </div>
      {/if}
      <div class="subagent-settings">
        <div class="settings-list">
          <label class="setting-row">
            <div><strong>{t('settings.subagent')}</strong><p>{t('settings.subagent.detail')}</p></div>
            <input type="checkbox" checked={settings.subagentEnabled !== false} disabled={saving} on:change={(event) => update({ subagentEnabled: event.currentTarget.checked })} />
            <span class="switch" aria-hidden="true"></span>
          </label>
        </div>
        <div class="auxiliary-endpoint-form">
          <label class="wide"><span>{t('settings.subagent.executionModel')}</span>
            <select value={subagentExecutionKey} disabled={saving || settings.subagentEnabled === false} on:change={(event) => selectSubagentExecutionModel(event.currentTarget.value)}>
              <option value="">{t('settings.subagent.inherit')}</option>
              {#each providers.filter((model) => model.available) as model (`subagent:${model.providerId}:${model.modelId}`)}
                <option value={`${model.providerId}::${model.modelId}`}>{model.providerName} · {model.modelName}</option>
              {/each}
            </select>
          </label>
          <label class="wide"><span>{t('settings.subagent.executionEffort')}</span>
            <select value={settings.subagentExecutionEffort ?? 'lowest'} disabled={saving || settings.subagentEnabled === false} on:change={(event) => update({ subagentExecutionEffort: event.currentTarget.value as SettingsSnapshot['subagentExecutionEffort'] })}>
              <option value="lowest">{t('settings.subagent.lowest')}</option>
              {#each ['none', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max'] as effort}
                <option value={effort}>{t(`settings.providers.effort.${effort}`)}</option>
              {/each}
            </select>
          </label>
        </div>
        <p class="settings-section-detail">{t('settings.subagent.reasoning')}</p>
      </div>
      </div>
    </details>

    <details class="settings-section">
      <summary class="settings-section-summary"><div class="settings-section-title"><span>03</span><h2>{t('settings.behavior')}</h2></div><Icon name="chevron" size={14} /></summary>
      <div class="settings-section-body">
      <div class="settings-list">
        <div class="setting-row avatar-row">
          <div><strong>{t('settings.avatar')}</strong><p>{t('settings.avatar.detail')}</p></div>
          <div class="avatar-actions">
            {#if settings.userAvatar}<button class="avatar-remove" type="button" disabled={saving} on:click={() => update({ userAvatar: '' })}>{t('settings.avatar.remove')}</button>{/if}
            <label class="avatar-picker" title={t('settings.avatar.choose')}>
              <input type="file" accept="image/png,image/jpeg,image/webp" disabled={saving} on:change={chooseAvatar} />
              {#if settings.userAvatar}<img src={settings.userAvatar} alt="" />{:else}<span>{locale === 'zh-CN' ? '你' : 'U'}</span>{/if}
              <small>{t('settings.avatar.choose')}</small>
            </label>
          </div>
        </div>
        <div class="setting-row access-row">
          <div><strong>{t('settings.fullAccess')}</strong><p>{t('settings.fullAccess.detail')}</p></div>
          <span class="access-badge"><Icon name="shield" size={14} />{t('settings.fullAccess.active')}</span>
        </div>
        <label class="setting-row">
          <div><strong>{t('settings.taskManager')}</strong><p>{t('settings.taskManager.detail')}</p></div>
          <input type="checkbox" checked={settings.taskManager} disabled={saving} on:change={(event) => update({ taskManager: event.currentTarget.checked })} />
          <span class="switch" aria-hidden="true"></span>
        </label>
        <label class="setting-row">
          <div><strong>{t('settings.caveman')}</strong><p>{t('settings.caveman.detail')}</p></div>
          <input type="checkbox" checked={settings.cavemanMode === 'lite'} disabled={saving} on:change={(event) => update({ cavemanMode: event.currentTarget.checked ? 'lite' : 'off' })} />
          <span class="switch" aria-hidden="true"></span>
        </label>
        <label class="setting-row">
          <div><strong>{t('settings.memory')}</strong><p>{t('settings.memory.detail')}</p></div>
          <input type="checkbox" checked={settings.memoryEnabled === true} disabled={saving} on:change={(event) => update({ memoryEnabled: event.currentTarget.checked })} />
          <span class="switch" aria-hidden="true"></span>
        </label>
        <div class="setting-row locale-row">
          <div><strong>{t('settings.language')}</strong><p>{t('settings.language.detail')}</p></div>
          <div class="segmented">
            <button class:active={locale === 'en-US'} type="button" disabled={saving} on:click={() => update({ locale: 'en-US' })}>{t('settings.language.en')}</button>
            <button class:active={locale === 'zh-CN'} type="button" disabled={saving} on:click={() => update({ locale: 'zh-CN' })}>{t('settings.language.zh')}</button>
          </div>
        </div>
        <div class="setting-row scale-row">
          <div><strong>{t('settings.uiScale')}</strong><p>{t('settings.uiScale.detail')}</p></div>
          <div class="scale-picker">
            <input
              type="range"
              min="0.85"
              max="1.30"
              step="0.05"
              value={settings.uiScale ?? 1}
              disabled={saving}
              style={`background: linear-gradient(90deg, #d9d9d9 ${scaleFill}%, #3a3a3a ${scaleFill}%)`}
              on:input={(event) => previewScale(Number(event.currentTarget.value))}
              on:change={(event) => update({ uiScale: Number(event.currentTarget.value) })}
              aria-label={t('settings.uiScale')}
            />
            <span class="scale-value">{Math.round((settings.uiScale ?? 1) * 100)}%</span>
          </div>
        </div>
      </div>
      </div>
    </details>
  </div>
</section>
