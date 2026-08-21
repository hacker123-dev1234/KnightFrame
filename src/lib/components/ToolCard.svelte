<script lang="ts">
  import Icon from './Icon.svelte';
  import StatusMark from './StatusMark.svelte';
  import type { Locale, ToolReceipt } from '../types';
  import { duration, translate } from '../i18n';
  import { presentTool } from '../toolPresentation';

  export let locale: Locale;
  export let tool: ToolReceipt;

  let expanded = false;
  $: t = (key: string) => translate(locale, key);
  $: presentation = presentTool(locale, tool);
  $: normalizedName = tool.name.trim().toLocaleLowerCase();
  $: icon = tool.status === 'failed'
    ? 'alert' as const
    : normalizedName === 'edit'
      ? 'edit' as const
      : normalizedName === 'read'
        ? 'panel' as const
        : normalizedName === 'task'
          ? 'tasks' as const
          : normalizedName === 'find' || normalizedName === 'search'
            ? 'graph' as const
            : 'spark' as const;
  $: hasDetails = presentation.facts.length > 0
    || presentation.blocks.length > 0
    || presentation.lists.length > 0
    || Boolean(tool.diagnostic || tool.artifactId || presentation.truncated);

  function toggle() {
    if (hasDetails) expanded = !expanded;
  }
</script>

<section class:running={tool.status === 'running'} class:failed={tool.status === 'failed'} class:expanded class="tool-card">
  <button
    class="tool-heading"
    class:static={!hasDetails}
    type="button"
    on:click={toggle}
    aria-expanded={hasDetails ? expanded : undefined}
    aria-controls={hasDetails ? `tool-details-${tool.id}` : undefined}
    aria-label={hasDetails
      ? `${t(expanded ? 'tool.collapse' : 'tool.expand')}: ${presentation.name}`
      : `${presentation.name}: ${presentation.summary}`}
  >
    <span class="tool-icon"><Icon name={icon} size={16} /></span>
    <span class="tool-name">
      <small>{t('tool.title')}</small>
      <strong>{presentation.name}</strong>
      <span class="tool-summary">{presentation.summary}</span>
    </span>
    {#if tool.elapsedMs !== undefined}<span class="tool-time">{duration(locale, tool.elapsedMs)}</span>{/if}
    <StatusMark status={tool.status} label={t(`tool.state.${tool.status}`)} />
    <span class:open={expanded} class:hidden={!hasDetails} class="tool-chevron"><Icon name="chevron" size={14} /></span>
  </button>

  {#if expanded && hasDetails}
    <div class="tool-body" id={`tool-details-${tool.id}`}>
      {#if presentation.facts.length}
        <dl class="tool-facts">
          {#each presentation.facts as fact}
            <div><dt>{fact.label}</dt><dd class:mono={fact.mono}>{fact.value}</dd></div>
          {/each}
        </dl>
      {/if}

      {#each presentation.lists as list}
        <section class="tool-section">
          <h4>{list.label}</h4>
          <ol class="tool-list">
            {#each list.items as item}
              <li>
                <div><strong>{item.title}</strong>{#if item.meta}<span>{item.meta}</span>{/if}</div>
                {#if item.detail}<p>{item.detail}</p>{/if}
              </li>
            {/each}
          </ol>
        </section>
      {/each}

      {#each presentation.blocks as block}
        <section class:error={block.kind === 'error'} class="tool-section tool-output">
          <h4>{block.label}</h4>
          <pre>{block.value}</pre>
        </section>
      {/each}

      {#if presentation.truncated}<p class="tool-truncated"><Icon name="panel" size={14} />{t('tool.truncated')}</p>{/if}

      {#if tool.diagnostic || tool.artifactId}
        <details class="tool-diagnostic">
          <summary><Icon name="shield" size={14} />{t('tool.diagnostic')}<Icon name="chevron" size={13} /></summary>
          <div>
            {#if tool.diagnostic}<p>{t(tool.diagnostic)}</p>{/if}
            {#if tool.artifactId}<code>{tool.artifactId}</code>{/if}
          </div>
        </details>
      {/if}
    </div>
  {/if}
</section>
