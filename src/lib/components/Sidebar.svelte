<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import type { Locale, ProjectSnapshot, SessionSnapshot } from '../types';
  import { translate } from '../i18n';

  export let locale: Locale;
  export let collapsed = false;
  export let sessions: SessionSnapshot[] = [];
  export let activeSessionId: string | undefined;
  export let project: ProjectSnapshot | undefined;
  export let onToggle: () => void;
  export let onNew: () => void;
  export let onSelect: (id: string) => void;
  export let onRename: (id: string, title: string) => void;
  export let onDelete: (id: string) => void;
  export let onSettings: () => void;
  export let onBrowser: () => void;
  // 市场页入口已收进 MarketChartCard 的"进入工作台"按钮（工具卡片直出后
  // 才有入口），Sidebar 不再提供市场直达。
  export let onGraph: () => void;
  export let onStudio: () => void;
  export let onOpenProject: () => void;

  let menuFor: string | undefined;
  let renaming: string | undefined;
  let draft = '';
  let confirmDelete: string | undefined;
  let renameInput: HTMLInputElement;
  let sidebarMiddle: HTMLDivElement;
  let sessionMenu: HTMLDivElement;
  let menuTop = 0;

  function closeMenu() {
    menuFor = undefined;
    confirmDelete = undefined;
  }

  async function openMenu(id: string, anchor: HTMLButtonElement) {
    if (menuFor === id) {
      closeMenu();
      return;
    }

    menuFor = id;
    renaming = undefined;
    confirmDelete = undefined;
    await tick();

    const middleBounds = sidebarMiddle.getBoundingClientRect();
    const anchorBounds = anchor.getBoundingClientRect();
    const menuHeight = sessionMenu.offsetHeight;
    const gap = 4;
    const below = anchorBounds.bottom - middleBounds.top + gap;
    const above = anchorBounds.top - middleBounds.top - menuHeight - gap;
    const maxTop = Math.max(0, middleBounds.height - menuHeight);
    menuTop = Math.min(below + menuHeight <= middleBounds.height ? below : Math.max(0, above), maxTop);
  }

  async function beginRename(session: SessionSnapshot) {
    renaming = session.id;
    draft = session.title && session.title !== 'session.new' ? session.title : '';
    confirmDelete = undefined;
    await tick();
    renameInput?.focus();
    renameInput?.select();
  }

  function finishRename(session: SessionSnapshot) {
    const title = draft.trim();
    if (title && title !== session.title) onRename(session.id, title);
    renaming = undefined;
    closeMenu();
  }

  function requestDelete(id: string) {
    if (confirmDelete === id) {
      onDelete(id);
      closeMenu();
    } else {
      confirmDelete = id;
    }
  }

  $: t = (key: string, args: Record<string, string | number> = {}) => translate(locale, key, args);
  $: menuSession = menuFor ? sessions.find((session) => session.id === menuFor) : undefined;
  $: if (collapsed) closeMenu();
</script>

<svelte:window on:resize={closeMenu} />

<div class:collapsed class="sidebar-shell">
  <aside class="sidebar" aria-label={t('sidebar.recent')}>
    <div class="sidebar-clip">
      <button
        class="brand-lockup"
        type="button"
        on:click={onToggle}
        aria-label={t(collapsed ? 'app.expand' : 'app.collapse')}
        title={t(collapsed ? 'app.expand' : 'app.collapse')}
      >
        <img src="/brand/knightframe-primary-white.png" alt="" />
        <strong>{t('app.brand')}</strong>
      </button>

      <button class="new-button" type="button" on:click={onNew} aria-label={t('sidebar.new')} title={t('sidebar.new')}>
        <Icon name="plus" size={18} />
        <span>{t('sidebar.new')}</span>
      </button>

      <div class="sidebar-middle" bind:this={sidebarMiddle} aria-hidden={collapsed} inert={collapsed}>
        <div class="section-label">{t('sidebar.recent')}</div>
        <nav class="session-list" on:scroll={closeMenu}>
          {#if sessions.length === 0}
            <p class="empty-list">{t('sidebar.empty')}</p>
          {:else}
            {#each sessions as session (session.id)}
              <div class:active={session.id === activeSessionId} class="session-row">
                {#if renaming === session.id}
                  <form class="session-rename" on:submit|preventDefault={() => finishRename(session)}>
                    <input bind:this={renameInput} bind:value={draft} aria-label={t('session.rename')} maxlength="64" on:keydown={(event) => event.key === 'Escape' && (renaming = undefined)} />
                    <button type="submit" aria-label={t('session.save')} title={t('session.save')}><Icon name="check" size={14} /></button>
                  </form>
                {:else}
                  <button class="session-item" type="button" on:click={() => onSelect(session.id)}>
                    <span class="session-title">{session.title ? t(session.title) : t('header.untitled')}</span>
                    <span class="session-state" class:streaming={session.status === 'streaming'}></span>
                  </button>
                  <button
                    class="session-more"
                    type="button"
                    on:click|stopPropagation={(event) => void openMenu(session.id, event.currentTarget)}
                    aria-label={t('session.options')}
                    aria-haspopup="menu"
                    aria-expanded={menuFor === session.id}
                    title={t('session.options')}
                  ><Icon name="more" size={16} /></button>
                {/if}
              </div>
            {/each}
          {/if}
        </nav>
        {#if menuSession}
          <div class="session-menu" bind:this={sessionMenu} style:top={`${menuTop}px`} role="menu" aria-label={t('session.options')}>
            <button type="button" role="menuitem" on:click={() => beginRename(menuSession)}><Icon name="edit" size={14} />{t('session.rename')}</button>
            <button class:confirm={confirmDelete === menuSession.id} type="button" role="menuitem" on:click={() => requestDelete(menuSession.id)}><Icon name="trash" size={14} />{t(confirmDelete === menuSession.id ? 'session.deleteConfirm' : 'session.delete')}</button>
          </div>
        {/if}
      </div>

      <nav class="collapsed-session-list" aria-label={t('sidebar.recent')} aria-hidden={!collapsed} inert={!collapsed}>
        {#each sessions as session, index (session.id)}
          <button
            class:active={session.id === activeSessionId}
            class:streaming={session.status === 'streaming'}
            type="button"
            on:click={() => onSelect(session.id)}
            aria-label={session.title ? t(session.title) : t('header.untitled')}
            title={session.title ? t(session.title) : t('header.untitled')}
            tabindex={collapsed ? 0 : -1}
          >
            <span>{String(index + 1).padStart(2, '0')}</span>
            <i></i>
          </button>
        {/each}
      </nav>

      <div class="sidebar-bottom">
        {#if project}
          <div class="project-receipt" aria-hidden={collapsed}><span>{t('sidebar.index')}</span><strong>{t(`sidebar.index.${project.status}`)}</strong></div>
        {/if}
        <button type="button" on:click={onOpenProject} aria-label={t('project.open')} title={t('project.open')}>
          <Icon name="panel" size={20} /><span>{project?.name ?? t('project.open')}</span>
        </button>
        {#if project}
          <button type="button" on:click={onGraph} aria-label={t('graph.title')} title={t('graph.title')}>
            <Icon name="graph" size={20} /><span>{t('graph.title')}</span>
          </button>
        {/if}
        <button type="button" on:click={onBrowser} aria-label={t('app.browser')} title={t('app.browser')}>
          <Icon name="browser" size={20} /><span>{t('app.browser')}</span>
        </button>
        <button type="button" on:click={onStudio} aria-label={t('app.plugins')} title={t('app.plugins')}>
          <Icon name="spark" size={20} /><span>{t('app.plugins')}</span>
        </button>
        <button type="button" on:click={onSettings} aria-label={t('app.settings')} title={t('app.settings')}>
          <Icon name="settings" size={20} /><span>{t('app.settings')}</span>
        </button>
      </div>
    </div>
  </aside>
</div>
