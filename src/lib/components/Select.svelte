<script lang="ts">
  // 统一黑白视觉的自绘下拉：替代原生 <select>（WebView2 下原生下拉
  // 是系统样式且偶发点击无响应），键盘与焦点行为保持可用。
  export let value = '';
  export let options: { value: string; label: string }[] = [];
  export let disabled = false;
  export let ariaLabel = '';

  let open = false;
  let root: HTMLElement | undefined;

  $: current = options.find((item) => item.value === value);

  function pick(next: string) {
    value = next;
    open = false;
  }

  function onButtonKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') open = false;
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      if (!disabled) open = !open;
    }
    if (!open) return;
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      const index = options.findIndex((item) => item.value === value);
      const delta = event.key === 'ArrowDown' ? 1 : -1;
      const next = options[(index + delta + options.length) % options.length];
      if (next) pick(next.value);
    }
  }

  function optionKeydown(event: KeyboardEvent, itemValue: string) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      pick(itemValue);
    }
  }
</script>

<svelte:window
  on:mousedown={(event) => {
    if (open && root && !root.contains(event.target as Node)) open = false;
  }}
/>

<div class="kf-select" class:open bind:this={root}>
  <button
    type="button"
    {disabled}
    class="kf-select-button"
    aria-haspopup="listbox"
    aria-expanded={open}
    aria-label={ariaLabel || current?.label}
    on:click={() => !disabled && (open = !open)}
    on:keydown={onButtonKeydown}
  >
    <span class="kf-select-value">{current?.label ?? value}</span>
    <span class="kf-select-chev" aria-hidden="true">
      <svg width="9" height="6" viewBox="0 0 9 6" fill="none">
        <path d="M1 1l3.5 3.5L8 1" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    </span>
  </button>
  {#if open}
    <ul class="kf-select-menu" role="listbox" aria-label={ariaLabel}>
      {#each options as item (item.value)}
        <li
          role="option"
          tabindex="-1"
          aria-selected={item.value === value}
          class:active={item.value === value}
          on:click={() => pick(item.value)}
          on:keydown={(event) => optionKeydown(event, item.value)}
        >
          {item.label}
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .kf-select {
    position: relative;
    width: 100%;
    min-width: 0;
  }
  .kf-select-button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: 10px;
    outline: none;
    color: var(--text);
    background: #0d0d0d;
    font: 12px var(--mono);
    text-align: left;
    cursor: pointer;
    transition: border-color 0.4s var(--ease-soft), box-shadow 0.5s var(--ease-soft), background-color 0.4s var(--ease-soft);
  }
  .kf-select-button:hover:not(:disabled) {
    border-color: #3d3d3d;
    background: #141414;
  }
  .kf-select-button:focus-visible,
  .kf-select.open .kf-select-button {
    border-color: rgba(255, 255, 255, 0.26);
    box-shadow: 0 0 18px rgba(255, 255, 255, 0.05);
  }
  .kf-select-button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .kf-select-value {
    overflow: hidden;
    min-width: 0;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .kf-select-chev {
    display: grid;
    flex: 0 0 auto;
    place-items: center;
    color: var(--muted);
    transition: transform 0.35s var(--ease-out), color 0.35s var(--ease-soft);
  }
  .kf-select.open .kf-select-chev {
    transform: rotate(180deg);
    color: var(--text-soft);
  }
  .kf-select-menu {
    position: absolute;
    z-index: 60;
    top: calc(100% + 5px);
    left: 0;
    max-height: 264px;
    margin: 0;
    padding: 5px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: rgba(16, 16, 16, 0.97);
    box-shadow: 0 18px 44px rgba(0, 0, 0, 0.6), 0 0 22px rgba(255, 255, 255, 0.03);
    backdrop-filter: blur(10px);
    list-style: none;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: #2d2d2d transparent;
    animation: kfSelectIn 0.22s var(--ease-out) both;
    min-width: 100%;
  }
  @keyframes kfSelectIn {
    from { opacity: 0; transform: translateY(-4px); }
    to { opacity: 1; transform: none; }
  }
  .kf-select-menu li {
    padding: 7px 10px;
    border-radius: 8px;
    color: var(--muted);
    font: 12px var(--mono);
    white-space: nowrap;
    cursor: pointer;
    transition: color 0.25s var(--ease-soft), background-color 0.25s var(--ease-soft);
  }
  .kf-select-menu li:hover {
    color: var(--text);
    background: rgba(255, 255, 255, 0.055);
  }
  .kf-select-menu li.active {
    color: var(--text);
    background: rgba(255, 255, 255, 0.045);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.07);
  }
</style>
