<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import Icon from '../components/Icon.svelte';
  import { bridge } from '../bridge';
  import type { BrowserAction, BrowserSnapshot, Locale } from '../types';
  import { translate } from '../i18n';

  export let locale: Locale;
  export let browser: BrowserSnapshot | undefined;
  export let onBackToWorkspace: () => void;
  export let onCommand: (action: BrowserAction, url?: string, tabId?: string) => Promise<void>;

  let address = browser?.url ?? '';
  let working = false;
  let stage: HTMLDivElement | undefined;
  let observer: ResizeObserver | undefined;
  $: t = (key: string) => translate(locale, key);
  $: activeTab = browser?.tabs?.find((tab) => tab.id === browser?.activeTabId);
  $: if (browser?.url && browser.url !== address && document.activeElement?.id !== 'browser-address') address = browser.url;

  const bookmarks: { label: string; url: string; hint: string }[] = [
    { label: 'Wikipedia', url: 'https://en.wikipedia.org', hint: 'en.wikipedia.org/wiki' },
    { label: 'Hacker News', url: 'https://news.ycombinator.com', hint: 'news.ycombinator.com' },
    { label: 'GitHub', url: 'https://github.com', hint: 'github.com' },
    { label: 'MDN', url: 'https://developer.mozilla.org', hint: 'developer.mozilla.org' },
  ];

  async function command(action: BrowserAction, url?: string, tabId?: string) {
    working = true;
    try { await onCommand(action, url, tabId); } finally { working = false; }
  }

  function tabTitle(title?: string, url?: string) {
    if (title?.trim()) return title.trim();
    if (url) {
      try { return new URL(url).hostname.replace(/^www\./, '') || t('browser.newTab'); } catch { /* use fallback */ }
    }
    return t('browser.newTab');
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Enter' && address.trim()) void command(browser?.open ? 'navigate' : 'open', address.trim());
  }

  // —— 内嵌浏览器：把舞台物理像素矩形同步给后端，子 webview 精确覆盖舞台 ——
  function syncRect() {
    if (!stage) return;
    const rect = stage.getBoundingClientRect();
    const ratio = window.devicePixelRatio || 1;
    void bridge.browserRect(
      Math.round(rect.left * ratio),
      Math.round(rect.top * ratio),
      Math.round(rect.width * ratio),
      Math.round(rect.height * ratio),
    ).catch(() => undefined);
  }

  onMount(() => {
    syncRect();
    observer = new ResizeObserver(() => syncRect());
    if (stage) observer.observe(stage);
    window.addEventListener('resize', syncRect);
  });
  onDestroy(() => {
    window.removeEventListener('resize', syncRect);
    observer?.disconnect();
    void bridge.browserCommand('hide').catch(() => undefined);
  });
  // 打开/导航后立即对齐一次（webview 可能刚被创建）
  $: if (browser?.open) syncRect();
</script>

<section class="browser-page">
  <div class="browser-tabs">
    <button class="browser-exit" type="button" on:click={onBackToWorkspace} aria-label={t('browser.backToWork')} title={t('browser.backToWork')}><Icon name="back" size={15} /></button>
    <div class="browser-tab-strip" role="tablist" aria-label={t('browser.tabs')}>
      {#each browser?.tabs ?? [] as tab (tab.id)}
        <div class:active={tab.id === browser?.activeTabId} class:loading={tab.loading} class="browser-tab">
          <button class="browser-tab-select" type="button" role="tab" aria-selected={tab.id === browser?.activeTabId} on:click={() => command('select-tab', undefined, tab.id)}>
            <span class="tab-state" aria-hidden="true"></span>
            <span>{tabTitle(tab.title, tab.url)}</span>
          </button>
          <button class="browser-tab-close" type="button" on:click={() => command('close-tab', undefined, tab.id)} aria-label={t('browser.closeTab')} title={t('browser.closeTab')}><Icon name="close" size={12} /></button>
        </div>
      {/each}
    </div>
    <button class="browser-new-tab" type="button" disabled={working} on:click={() => command('new-tab')} aria-label={t('browser.newTab')} title={t('browser.newTab')}><Icon name="plus" size={15} /></button>
  </div>
  <div class="browser-toolbar">
    <div class="browser-nav">
      <button type="button" disabled={!activeTab?.canGoBack || working} on:click={() => command('back')} aria-label={t('app.back')}><Icon name="back" /></button>
      <button type="button" disabled={!activeTab?.canGoForward || working} on:click={() => command('forward')} aria-label={t('app.forward')}><Icon name="forward" /></button>
    </div>
    <div class:loading={browser?.loading} class="address-bar">
      <Icon name="shield" size={15} />
      <input id="browser-address" bind:value={address} on:keydown={keydown} placeholder={t('browser.address')} aria-label={t('browser.address')} />
      <button type="button" disabled={!address.trim() || working} on:click={() => command(browser?.open ? 'navigate' : 'open', address.trim())} aria-label={t('browser.open')}><Icon name="send" size={15} /></button>
    </div>
    <button type="button" disabled={!browser?.open || working} on:click={() => command(browser?.loading ? 'stop' : 'refresh')} aria-label={browser?.loading ? t('browser.stop') : t('app.refresh')} title={browser?.loading ? t('browser.stop') : t('app.refresh')}><Icon name={browser?.loading ? 'close' : 'refresh'} size={15} /></button>
  </div>

  <div class="browser-stage" bind:this={stage}>
    {#if !browser?.available}
      <div class="browser-state"><Icon name="browser" size={34} /><h1>{t('browser.title')}</h1><p>{t('browser.unavailable')}</p></div>
    {:else if browser.error}
      <div class="browser-state failed"><Icon name="alert" size={34} /><h1>{t('browser.title')}</h1><p>{t(browser.error.key)}</p></div>
    {:else if !browser.open}
      <div class="browser-welcome">
        <div class="welcome-aura" aria-hidden="true"></div>
        <div class="welcome-emblem" aria-hidden="true"><Icon name="browser" size={30} /></div>
        <h1>{t('browser.title')}</h1>
        <p class="welcome-sub">{t('browser.welcome')}</p>
        <div class="welcome-bookmarks">
          {#each bookmarks as bookmark (bookmark.url)}
            <button type="button" disabled={working} on:click={() => command('new-tab', bookmark.url)}>
              <strong>{bookmark.label}</strong>
              <small>{bookmark.hint}</small>
            </button>
          {/each}
        </div>
        <p class="welcome-agent"><Icon name="spark" size={13} />{t('browser.agentHint')}</p>
      </div>
    {:else}
      <div class="webview-contract" aria-live="polite">
        {#if browser.loading}
          <div class="browser-loading"><span class="loading-bar"></span>{t('browser.loading')}</div>
        {:else if browser.url}
          <div class="browser-viewing">
            <span class="viewing-label">{t('browser.pageReady')}</span>
            <strong>{browser.url}</strong>
          </div>
        {/if}
      </div>
    {/if}
  </div>

  {#if browser?.permissions?.length}
    <div class="permission-receipts">
      <strong>{t('browser.permissions')}</strong>
      {#each browser.permissions as permission (permission.id)}
        <span><Icon name="shield" size={13} />{permission.permission} · {t(`browser.permission.${permission.status}`)}</span>
      {/each}
    </div>
  {/if}
</section>

<style>
  .browser-welcome {
    position: relative;
    display: grid;
    place-items: center;
    gap: 6px;
    width: min(480px, calc(100% - 56px));
    padding: 44px 20px 40px;
    text-align: center;
  }
  .welcome-aura {
    position: absolute;
    width: 340px;
    height: 340px;
    border-radius: 50%;
    background: radial-gradient(circle, rgba(255, 255, 255, 0.09), rgba(255, 255, 255, 0.02) 42%, transparent 70%);
    filter: blur(6px);
    animation: welcomeBreath 6.5s var(--ease-soft) infinite;
    pointer-events: none;
  }
  @keyframes welcomeBreath {
    0%, 100% { opacity: 0.5; transform: scale(0.94); }
    50% { opacity: 1; transform: scale(1.05); }
  }
  .welcome-emblem {
    position: relative;
    display: grid;
    place-items: center;
    width: 68px;
    height: 68px;
    border: 1px solid var(--border);
    border-radius: 22px;
    color: var(--text-soft);
    background: linear-gradient(150deg, #161616, #0d0d0d);
    box-shadow: 0 0 34px rgba(255, 255, 255, 0.06), inset 0 1px rgba(255, 255, 255, 0.05);
    animation: emblemBreath 5.4s var(--ease-soft) infinite;
  }
  @keyframes emblemBreath {
    0%, 100% { box-shadow: 0 0 26px rgba(255, 255, 255, 0.045), inset 0 1px rgba(255, 255, 255, 0.05); }
    50% { box-shadow: 0 0 44px rgba(255, 255, 255, 0.1), inset 0 1px rgba(255, 255, 255, 0.08); }
  }
  .browser-welcome h1 {
    position: relative;
    margin: 16px 0 4px;
    color: var(--text);
    font: 400 32px/1.15 var(--serif);
    letter-spacing: -0.03em;
  }
  .welcome-sub {
    position: relative;
    margin: 0;
    color: var(--muted);
    font-size: 14px;
  }
  .welcome-bookmarks {
    position: relative;
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px;
    margin-top: 22px;
  }
  .welcome-bookmarks button {
    display: grid;
    gap: 4px;
    min-width: 128px;
    padding: 11px 14px;
    border: 1px solid var(--border-soft);
    border-radius: 14px;
    color: var(--text-soft);
    background: #0d0d0d;
    cursor: pointer;
    text-align: left;
    transition: border-color 0.45s var(--ease-soft), background-color 0.45s var(--ease-soft), box-shadow 0.6s var(--ease-soft), transform 0.45s var(--ease-out);
  }
  .welcome-bookmarks button:hover:not(:disabled) {
    border-color: var(--border);
    background: #141414;
    transform: translateY(-2px);
    box-shadow: 0 0 24px rgba(255, 255, 255, 0.06);
  }
  .welcome-bookmarks strong { font: 600 12px var(--serif); }
  .welcome-bookmarks small { color: var(--dim); font: 10px var(--mono); }
  .welcome-agent {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    margin: 26px 0 0;
    padding: 9px 14px;
    border: 1px dashed var(--border-soft);
    border-radius: 999px;
    color: var(--muted);
    background: rgba(255, 255, 255, 0.015);
    font: 11px/1.5 var(--serif);
  }
  .browser-loading {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 14px;
    border: 1px solid var(--border);
    border-radius: 10px;
    color: var(--muted);
    background: rgba(10, 10, 10, 0.92);
    font: 10px var(--mono);
  }
  .loading-bar {
    width: 34px;
    height: 2px;
    border-radius: 2px;
    background: rgba(255, 255, 255, 0.12);
    overflow: hidden;
    position: relative;
  }
  .loading-bar::after {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    width: 14px;
    border-radius: inherit;
    background: #fff;
    box-shadow: 0 0 8px rgba(255, 255, 255, 0.65);
    animation: loadingSweep 1.15s var(--ease-soft) infinite;
  }
  @keyframes loadingSweep {
    from { left: -16px; }
    to { left: 36px; }
  }
  .browser-viewing {
    display: flex;
    align-items: center;
    gap: 12px;
    max-width: min(560px, calc(100% - 48px));
    padding: 10px 16px;
    border: 1px solid var(--border-soft);
    border-radius: 12px;
    color: var(--muted);
    background: var(--surface-1);
  }
  .viewing-label {
    flex: 0 0 auto;
    color: var(--dim);
    font: 600 9px var(--serif);
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }
  .browser-viewing strong {
    overflow: hidden;
    color: var(--text-soft);
    font: 500 12px var(--serif);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
