<script lang="ts">
  import Icon from '../components/Icon.svelte';
  import GraphCanvas from '../components/GraphCanvas.svelte';
  import type { GraphSnapshot, Locale, LocalizedError } from '../types';
  import { translate } from '../i18n';

  export let locale: Locale;
  export let graph: GraphSnapshot | undefined;
  export let loading = false;
  export let error: LocalizedError | undefined;
  export let onBack: () => void;
  export let onRefresh: () => void;

  let panelOpen = true;
  let query = '';
  let showFiles = true;
  let showDirectories = true;
  let nodeScale = 1;
  let linkStrength = 1;
  let repulsion = 1;
  $: t = (key: string) => translate(locale, key);
</script>

<section class="graph-page">
  <header class="graph-header" data-tauri-drag-region>
    <button type="button" on:click={onBack} aria-label={t('app.back')} title={t('app.back')}><Icon name="back" /></button>
    <div><span>{t('graph.eyebrow')}</span><h1>{t('graph.title')}</h1></div>
    {#if graph}<div class="graph-stats"><span>{graph.stats.components} {t('graph.components')}</span><span>{graph.stats.files} {t('graph.files')}</span><span>{graph.stats.dependencies} {t('graph.dependencies')}</span></div>{/if}
    <button type="button" on:click={() => (panelOpen = !panelOpen)} class:active={panelOpen} aria-label={t('graph.controls')} title={t('graph.controls')}><Icon name="settings" /></button>
    <button type="button" on:click={onRefresh} disabled={loading} aria-label={t('graph.refresh')} title={t('graph.refresh')}><Icon name="refresh" /></button>
  </header>
  <div class="graph-stage">
    {#if loading}
      <div class="graph-loading"><span></span><Icon name="graph" size={32} /><p>{t('graph.loading')}</p></div>
    {:else if error}
      <div class="graph-empty"><Icon name="alert" size={32} /><p>{t(error.key)}</p><button type="button" on:click={onRefresh}>{t('app.retry')}</button></div>
    {:else if graph}
      {#key `${graph.root}:${query}:${showFiles}:${showDirectories}:${nodeScale}:${linkStrength}:${repulsion}`}
        <GraphCanvas {graph} {query} {showFiles} {showDirectories} {nodeScale} {linkStrength} {repulsion} label={t('graph.title')} />
      {/key}
    {:else}
      <div class="graph-empty"><Icon name="graph" size={32} /><p>{t('graph.empty')}</p></div>
    {/if}
    {#if panelOpen && graph}
      <aside class="graph-controls">
        <label class="graph-search"><Icon name="spark" size={14} /><input bind:value={query} placeholder={t('graph.search')} /></label>
        <section><h2>{t('graph.filters')}</h2><label><input type="checkbox" bind:checked={showDirectories} /><span></span>{t('graph.directories')}</label><label><input type="checkbox" bind:checked={showFiles} /><span></span>{t('graph.files')}</label></section>
        <section><h2>{t('graph.display')}</h2><label class="graph-range">{t('graph.nodeSize')}<input type="range" min="0.65" max="1.7" step="0.05" bind:value={nodeScale} /></label></section>
        <section><h2>{t('graph.forces')}</h2><label class="graph-range">{t('graph.linkForce')}<input type="range" min="0.35" max="2" step="0.05" bind:value={linkStrength} /></label><label class="graph-range">{t('graph.repelForce')}<input type="range" min="0.35" max="2" step="0.05" bind:value={repulsion} /></label></section>
        <small>{t('graph.hint')}</small>
      </aside>
    {/if}
  </div>
</section>
