<script lang="ts">
  // 工坊 KF 预览覆盖层：真实 KnightFrame 界面上的插件组件（可交互）。
  import { fade } from 'svelte/transition';
  import { studioPreviewComponents, studioPreviewRendering, studioPreviewStyle } from '../studioPreview';
  import type { StudioComponent } from '../studio';

  const UI_PAGES = ['workspace', 'market', 'browser', 'settings', 'graph', 'studio'];
  let banner = '';
  let bannerTimer: ReturnType<typeof setTimeout> | undefined;
  let toggles: Record<string, boolean> = {};

  function notify(text: string) {
    banner = text;
    if (bannerTimer) clearTimeout(bannerTimer);
    bannerTimer = setTimeout(() => (banner = ''), 2600);
  }

  function activate(item: StudioComponent) {
    let matched = false;
    for (const action of item.actions.filter((candidate) => candidate.event === 'click')) {
      matched = true;
      if (action.action === 'notify') notify(action.value || '✦');
      if (action.action === 'openUrl' && action.value) {
        window.parent?.postMessage({ type: 'kf-studio-open-url', url: action.value }, '*');
        notify(`↗ ${action.value}`);
      }
      // 驱动 KnightFrame 原生 UI 层：工坊经后端转发到主窗口切页面
      if (action.action === 'openPage' && UI_PAGES.includes(action.value.trim())) {
        window.parent?.postMessage({ type: 'kf-studio-open-page', page: action.value.trim() }, '*');
        notify(`⤴ ${action.value.trim()}`);
      }
      if (action.action === 'command') notify(`⚙ ${action.value || item.props.command || 'command'}`);
      if (action.action === 'toggle') toggles = { ...toggles, [item.id]: !toggles[item.id] };
    }
    if (!matched && item.type === 'toggle') toggles = { ...toggles, [item.id]: !toggles[item.id] };
  }
</script>

{#if $studioPreviewRendering}<div class="kf-preview-layer" aria-label="Plugin Studio preview">
  <div class="kf-preview-stage">
    <div class="kf-preview-surface kf-plugin-surface">
      {#each $studioPreviewComponents as item (item.id)}
        <div class="kf-preview-node kf-plugin-component" style={studioPreviewStyle(item)} role="presentation">
          {#if item.type === 'button'}
            <button type="button" disabled={item.props.disabled} on:click={() => activate(item)}>{item.props.label}</button>
          {:else if item.type === 'toggle'}
            <button class="kf-preview-toggle" type="button" class:on={toggles[item.id]} disabled={item.props.disabled} on:click={() => activate(item)}>
              <span>{item.props.label}</span><i></i>
            </button>
          {:else if item.type === 'text'}
            <p>{item.props.label}</p>
          {:else if item.type === 'input'}
            <input placeholder={item.props.label} disabled={item.props.disabled} />
          {:else if item.type === 'select'}
            <select disabled={item.props.disabled}>
              {#each item.props.options ?? [] as option, index (index)}<option value={`option-${index + 1}`}>{option}</option>{/each}
            </select>
          {:else if item.type === 'separator'}
            <hr />
          {:else}
            <section><strong>{item.props.label}</strong></section>
          {/if}
        </div>
      {/each}
    </div>
  </div>
  {#if banner}
    <div class="kf-preview-banner" transition:fade={{ duration: 180 }} role="status">{banner}</div>
  {/if}
</div>{/if}

<style>
  .kf-preview-layer { position:fixed; z-index:120; inset:0; pointer-events:none; }
  /* 16:9 参考舞台（1280×720 基准）：与工坊画布同比例、同基准字号（cqw 容器查询），
     工坊里 46px 高的按钮在这里就是原生 KF 按钮的实际大小。 */
  .kf-preview-stage { position:absolute; left:50%; top:50%; transform:translate(-50%,-50%); width:min(96vw, 1600px, 171vh); aspect-ratio:16/9; container-type:inline-size; }
  .kf-preview-surface { position:absolute; inset:0; font-size:0.9375cqw; }
  .kf-preview-node { position:absolute; display:grid; place-items:stretch; min-width:10px; min-height:6px; pointer-events:auto; cursor:pointer; }
  /* 子控件全部继承节点内联样式（字号/颜色/背景/圆角/边框随工坊滑条即时变化）；
     悬停/按压手感与 KF 原版按钮一致（上浮 + 白光 + scale .965）。 */
  .kf-preview-node button, .kf-preview-node input, .kf-preview-node select { width:100%; height:100%; min-width:0; border:1px solid; border-color:inherit; border-radius:inherit; padding:inherit; outline:none; color:inherit; background:inherit; font:inherit; text-align:inherit; cursor:pointer; transition:color .5s cubic-bezier(.4,0,.2,1),border-color .5s cubic-bezier(.4,0,.2,1),background-color .5s cubic-bezier(.4,0,.2,1),box-shadow .65s cubic-bezier(.4,0,.2,1),transform .5s cubic-bezier(.22,1,.36,1); }
  .kf-preview-node button:hover:not(:disabled) { border-color:#303030; background:#1a1a1a; box-shadow:0 0 18px rgba(255,255,255,.055); transform:translateY(-1px); }
  .kf-preview-node button:active:not(:disabled) { transform:translateY(0) scale(.965); }
  .kf-preview-node button:disabled, .kf-preview-node input:disabled, .kf-preview-node select:disabled { cursor:not-allowed; opacity:.38; }
  .kf-preview-node button:focus-visible, .kf-preview-node input:focus-visible, .kf-preview-node select:focus-visible { outline:1px solid rgba(255,255,255,.72); outline-offset:2px; }
  .kf-preview-node p { display:flex; align-items:center; margin:0; color:inherit; font-size:inherit; font-weight:inherit; text-align:inherit; }
  .kf-preview-node hr { width:100%; height:1px; margin:auto 0; border:0; background:inherit; }
  .kf-preview-node section { display:grid; place-items:center; height:100%; border:1px solid; border-radius:inherit; color:inherit; background:inherit; box-shadow:inherit; font-size:inherit; }
  .kf-preview-toggle { display:flex; align-items:center; justify-content:space-between; gap:8px; padding:0 12% !important; text-align:left !important; }
  .kf-preview-toggle i { position:relative; width:30px; height:16px; border-radius:99px; background:#2e2e2e; transition:background .3s ease; }
  .kf-preview-toggle i::after { content:''; position:absolute; top:2px; left:2px; width:12px; height:12px; border-radius:50%; background:#bbb; transition:transform .3s ease, background .3s ease; }
  .kf-preview-toggle.on i { background:#4a4a4a; }
  .kf-preview-toggle.on i::after { transform:translateX(14px); background:#fff; box-shadow:0 0 8px rgba(255,255,255,.5); }
  .kf-preview-banner { position:fixed; right:22px; bottom:22px; max-width:360px; padding:10px 14px; border:1px solid #3d3d3d; border-radius:12px; color:#eee; background:rgba(14,14,14,.97); box-shadow:0 16px 44px rgba(0,0,0,.6); font:500 12px/1.5 var(--serif, serif); pointer-events:none; }
</style>
