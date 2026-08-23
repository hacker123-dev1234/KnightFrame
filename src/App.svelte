<script lang="ts">
  import { onMount } from 'svelte';
  import { app, activeSession, bootstrap, configureModelThinking, createSession, deleteSession, destroy, loadGraph, openProject, renameSession, runBrowserCommand, selectModel, selectSession, setPage, stopActive, submit, toggleBrowserPanel, toggleSidebar, toggleTasks, toggleUsage, updateSettings } from './lib/state';
  import { bridge } from './lib/bridge';
  import { translate } from './lib/i18n';
  import { applyUiScale } from './lib/uiScale';
  import Sidebar from './lib/components/Sidebar.svelte';
  import Header from './lib/components/Header.svelte';
  import TaskPanel from './lib/components/TaskPanel.svelte';
  import UsagePanel from './lib/components/UsagePanel.svelte';
  import IndexingGate from './lib/components/IndexingGate.svelte';
  import Icon from './lib/components/Icon.svelte';
  import EmptyPage from './lib/pages/EmptyPage.svelte';
  import ConversationPage from './lib/pages/ConversationPage.svelte';
  import SettingsPage from './lib/pages/SettingsPage.svelte';
  import BrowserPage from './lib/pages/BrowserPage.svelte';
  import MiniPage from './lib/pages/MiniPage.svelte';
  import GraphPage from './lib/pages/GraphPage.svelte';
  import MarketPage from './lib/pages/MarketPage.svelte';
  import StudioPreviewOverlay from './lib/components/StudioPreviewOverlay.svelte';
  import { initStudioPreview, studioPreviewActive } from './lib/studioPreview';
  import type { ProviderModel } from './lib/types';

  $: locale = $app.settings.locale;
  $: t = (key: string) => translate(locale, key);
  $: projectIndexing = $app.project?.status === 'indexing' || $app.project?.status === 'updating';
  $: canSend = $app.runtime === 'ready' && !projectIndexing && $app.providers.some((model) => model.available);
  $: cacheRead = $activeSession?.usage?.cacheReadTokens ?? 0;
  $: freshInput = $activeSession?.usage?.freshInputTokens ?? 0;
  $: cacheHitRate = cacheRead + freshInput > 0 ? Math.round((cacheRead / (cacheRead + freshInput)) * 100) : undefined;
  $: turnCacheRead = $activeSession?.usage?.turnCacheReadTokens ?? 0;
  $: turnFreshInput = $activeSession?.usage?.turnFreshInputTokens ?? 0;
  $: turnCacheHitRate = turnCacheRead + turnFreshInput > 0
    ? Math.round((turnCacheRead / (turnCacheRead + turnFreshInput)) * 100)
    : undefined;
  $: selectedProviderId = $activeSession?.providerId ?? $app.settings.providerId;
  $: selectedModelId = $activeSession?.modelId ?? $app.settings.modelId;
  $: selectedModel = $app.providers.find((model: ProviderModel) =>
    model.providerId === selectedProviderId && model.modelId === selectedModelId,
  );
  $: imageInput = selectedModel?.capabilities?.includes('imageInput') === true;
  $: contextTokens = typeof $activeSession?.usage?.currentContextTokens === 'number'
    ? $activeSession.usage.currentContextTokens
    : undefined;
  $: contextLimit = typeof selectedModel?.contextLimit === 'number' && selectedModel.contextLimit > 0
    ? selectedModel.contextLimit
    : undefined;
  $: contextUsageRate = contextTokens !== undefined && contextLimit !== undefined
    ? Math.min(100, Math.round((contextTokens / contextLimit) * 100))
    : undefined;
  $: contextUsageDetail = contextTokens !== undefined && contextLimit !== undefined
    ? translate(locale, 'usage.contextDetail', {
        used: new Intl.NumberFormat(locale).format(contextTokens),
        limit: new Intl.NumberFormat(locale).format(contextLimit),
      })
    : t('usage.unavailable');
  // 界面缩放：settings.uiScale 直接驱动根容器 zoom（WebView2 原生支持，
  // 布局/字体/图标整体随动，无需逐组件改 rem）
  $: uiScale = $app.settings.uiScale ?? 1;
  $: if (typeof document !== 'undefined') {
    document.documentElement.style.setProperty('zoom', String(uiScale));
  }
  const query = new URLSearchParams(location.search);
  const requestedView = window.__KF_BOOTSTRAP__?.view ?? query.get('view');
  $: miniMode = requestedView === 'mini';
  const studioPreview = initStudioPreview();
  let unsubscribePluginStudio: (() => void) | undefined;
  let browserDockWidth = 420;

  function resetBrowserDock() {
    browserDockWidth = 420;
  }

  function resizeBrowserDock(event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    const startX = event.clientX;
    const startWidth = browserDockWidth;
    const workspace = (event.currentTarget as HTMLElement).closest('.main-workspace') as HTMLElement | null;
    const maxWidth = Math.max(320, (workspace?.clientWidth ?? window.innerWidth) - 460);
    const move = (next: PointerEvent) => {
      browserDockWidth = Math.max(300, Math.min(maxWidth, startWidth + next.clientX - startX));
    };
    const end = () => {
      window.removeEventListener('pointermove', move);
      document.documentElement.classList.remove('resizing-browser-dock');
    };
    document.documentElement.classList.add('resizing-browser-dock');
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', end, { once: true });
  }

  onMount(() => {
    if (requestedView === 'settings' || requestedView === 'browser') setPage(requestedView);
    void bridge.subscribePluginStudio(async ({ content }) => {
      setPage('workspace');
      await submit(content);
    }).then((unsubscribe) => { unsubscribePluginStudio = unsubscribe; }).catch(() => undefined);
    void bootstrap();
    return () => {
      unsubscribePluginStudio?.();
      destroy();
    };
  });

  async function cancelTask() {
    const item = $activeSession?.task?.items.find((candidate) => candidate.status === 'running' || candidate.status === 'pending');
    if ($activeSession && item) await bridge.taskCommand($activeSession.id, 'cancelled', item.id);
  }

  async function openStudio() {
    await bridge.openPluginStudio();
  }

  function closeChildWindow() {
    void import('@tauri-apps/api/window').then(({ getCurrentWindow }) => getCurrentWindow().close());
  }

  function openTaskPanel() {
    if (!$app.taskPanelOpen) toggleTasks();
  }
</script>

{#if miniMode}
  <MiniPage
    {locale}
    available={$app.features?.miniAssistant === true}
    session={$activeSession}
    userAvatar={$app.settings.userAvatar}
    {imageInput}
    onClose={closeChildWindow}
    onSend={submit}
    onStop={stopActive}
  />
{:else}
  <div class:sidebar-collapsed={$app.sidebarCollapsed} class:has-inspector={$app.taskPanelOpen || $app.usagePanelOpen} class="app-shell">
    <div class="ambient ambient-one"></div><div class="ambient ambient-two"></div>
    <Sidebar
      {locale}
      collapsed={$app.sidebarCollapsed}
      sessions={$app.sessions}
      activeSessionId={$app.activeSessionId}
      project={$app.project}
      onToggle={toggleSidebar}
      onNew={() => void createSession()}
      onSelect={selectSession}
      onRename={(id, title) => void renameSession(id, title)}
      onDelete={(id) => void deleteSession(id)}
      onSettings={() => setPage('settings')}
      onBrowser={() => void toggleBrowserPanel()}
      onGraph={() => void loadGraph()}
      onStudio={() => void openStudio()}
      onOpenProject={openProject}
    />
    <main class="main-workspace" style={`--browser-dock-width:${browserDockWidth}px`}>
      {#if $app.page !== 'workspace'}<div class="page-drag-strip" data-tauri-drag-region aria-hidden="true"></div>{/if}
      {#if $app.page === 'workspace'}
        <Header
          {locale}
          session={$activeSession}
          models={$app.providers}
          project={$app.project}
          preferredProviderId={$app.settings.providerId}
          preferredModelId={$app.settings.modelId}
          taskActive={$app.taskPanelOpen}
          taskEnabled={$app.settings.taskManager}
          usageActive={$app.usagePanelOpen}
          onTasks={toggleTasks}
          onUsage={toggleUsage}
          onSelectModel={selectModel}
          onConfigureThinking={configureModelThinking}
        />
        <div class:browser-docked={$app.browserPanelOpen} class="workspace-content">
          {#if $app.browserPanelOpen}
            <section class="browser-dock" aria-label={t('browser.title')}>
              <BrowserPage {locale} browser={$app.browser} onBackToWorkspace={() => void toggleBrowserPanel(false)} onCommand={runBrowserCommand} />
            </section>
            <div class="browser-dock-resizer" role="separator" aria-orientation="vertical" aria-label={t('browser.resize')} title={t('browser.resizeHint')} on:pointerdown={resizeBrowserDock} on:dblclick={resetBrowserDock}><span></span></div>
          {/if}
          <div class="conversation-pane">
            {#if $activeSession}<ConversationPage {locale} session={$activeSession} userAvatar={$app.settings.userAvatar} {imageInput} disabled={!canSend} taskPanelOpen={$app.taskPanelOpen} onOpenTask={$app.settings.taskManager ? openTaskPanel : undefined} onSend={submit} onStop={stopActive} />
            {:else}<EmptyPage {locale} {canSend} {imageInput} onSend={submit} onStop={stopActive} />{/if}
          </div>
        </div>
        <div class:browser-docked={$app.browserPanelOpen} class="workspace-metrics" aria-label={t('usage.title')}>
          <div
            class:active={contextUsageRate !== undefined && contextUsageRate > 0}
            class="metric-hud context-usage-hud"
            aria-label={`${t('usage.contextUsed')}: ${contextUsageDetail}`}
            title={contextUsageDetail}
          >
            <span>{t('usage.contextUsed')}</span><strong>{contextUsageRate === undefined ? t('usage.unavailable') : `${contextUsageRate}%`}</strong>
          </div>
          <div
            class:active={turnCacheHitRate !== undefined && turnCacheHitRate > 0}
            class="metric-hud cache-hit-hud"
            aria-label={`${t('usage.cacheHitCurrent')}: ${turnCacheHitRate === undefined ? t('usage.unavailable') : `${turnCacheHitRate}%`}; ${t('usage.cacheHitSession')}: ${cacheHitRate === undefined ? t('usage.unavailable') : `${cacheHitRate}%`}`}
          >
            <span class="metric-copy"><span>{t('usage.cacheHitCurrent')}</span><small>{t('usage.cacheHitSession')} {cacheHitRate === undefined ? t('usage.unavailable') : `${cacheHitRate}%`}</small></span>
            <strong>{turnCacheHitRate === undefined ? t('usage.unavailable') : `${turnCacheHitRate}%`}</strong>
          </div>
        </div>
        {#if projectIndexing && $app.project}<IndexingGate {locale} project={$app.project} />{/if}
      {:else if $app.page === 'settings'}
        <SettingsPage settings={$app.settings} providers={$app.providers} providerTemplates={$app.providerTemplates} modelSelectionDisabled={$activeSession?.status === 'streaming'} onBack={() => setPage('workspace')} onUpdate={updateSettings} onSelectModel={selectModel} />
      {:else if $app.page === 'browser'}
        <BrowserPage {locale} browser={$app.browser} onBackToWorkspace={() => setPage('workspace')} onCommand={runBrowserCommand} />
      {:else if $app.page === 'graph'}
        <GraphPage {locale} graph={$app.graph} loading={$app.graphLoading} error={$app.graphError} onBack={() => setPage('workspace')} onRefresh={() => void loadGraph()} />
      {:else if $app.page === 'market'}
        <MarketPage {locale} />
      {:else}
        <MiniPage {locale} available={$app.features?.miniAssistant === true} session={$activeSession} userAvatar={$app.settings.userAvatar} {imageInput} onClose={() => setPage('workspace')} onSend={submit} onStop={stopActive} />
      {/if}
    </main>

    {#if $app.settings.taskManager && $app.taskPanelOpen}<TaskPanel {locale} task={$activeSession?.task} onClose={toggleTasks} onCancel={cancelTask} />{/if}
    {#if $app.usagePanelOpen}<UsagePanel {locale} usage={$activeSession?.usage} onClose={toggleUsage} />{/if}
    {#if studioPreview && $studioPreviewActive}<StudioPreviewOverlay />{/if}

    {#if $app.runtime !== 'ready'}
      <div class="runtime-banner" role="status">
        <span class:offline={$app.runtime === 'offline'}></span>
        <p>{t($app.runtime === 'connecting' ? 'runtime.connecting' : $app.error?.key ?? 'runtime.offline')}</p>
        {#if $app.runtime === 'offline'}<button type="button" on:click={() => bootstrap()}><Icon name="refresh" size={14} />{t('runtime.retry')}</button>{/if}
      </div>
    {/if}
  </div>
{/if}
