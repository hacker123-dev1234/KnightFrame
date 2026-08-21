<script lang="ts">
  import type { Locale, ProjectSnapshot } from '../types';
  import { translate } from '../i18n';
  import { fade } from 'svelte/transition';

  export let locale: Locale;
  export let project: ProjectSnapshot;

  $: t = (key: string) => translate(locale, key);
  $: updating = project.status === 'updating';
</script>

<section
  class="indexing-gate"
  role="status"
  aria-live="polite"
  aria-busy="true"
  aria-label={t(updating ? 'project.indexing.updatingTitle' : 'project.indexing.title')}
  out:fade={{ duration: 520 }}
>
  <div class="indexing-field" aria-hidden="true">
    <div class="indexing-orbit orbit-outer">
      {#each Array(8) as _, index}<i style={`--angle:${index * 45}deg`}></i>{/each}
      {#each [0, 90, 180, 270] as angle, index}
        <span class="indexing-strike" style={`--angle:${angle + 22.5}deg; --sd:${index * 0.7}s`}></span>
      {/each}
    </div>
    <div class="indexing-orbit orbit-inner">
      {#each Array(5) as _, index}<i style={`--angle:${index * 72}deg`}></i>{/each}
    </div>
    <div class="indexing-knight"><img src="/brand/knightframe-ui-hero-white.png" alt="" /></div>
  </div>
  <div class="indexing-copy">
    <small>{t('app.brand')}</small>
    <h2>{t(updating ? 'project.indexing.updatingTitle' : 'project.indexing.title')}</h2>
    <p>{t(updating ? 'project.indexing.updatingDetail' : 'project.indexing.detail')}</p>
    <span class="indexing-pulse" aria-hidden="true"><i></i><i></i><i></i></span>
  </div>
</section>
