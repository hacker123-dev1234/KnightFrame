// 插件工坊的 KnightFrame 宿主预览：index.html?studioPreview=1 时激活。
// 工坊画布通过 postMessage 推送布局，本层把插件组件渲染成真实界面上的
// 覆盖层（可点击：notify 弹横幅，openUrl 让工坊在主窗口打开内置浏览器）。
// 舞台固定 16:9（参考 1280×720）：与工坊画布、DSH 导出画布同比例同基准字号，
// 三个地方渲染出来的控件尺寸/字号完全一致。
import { writable } from 'svelte/store';
import type { StudioComponent } from './studio';
import { STUDIO_BASE_FONT } from './studio';

export const studioPreviewActive = writable(false);
export const studioPreviewComponents = writable<StudioComponent[]>([]);

export function initStudioPreview(): boolean {
  const params = new URLSearchParams(window.location.search);
  if (params.get('studioPreview') !== '1') return false;
  studioPreviewActive.set(true);
  window.addEventListener('message', (event: MessageEvent) => {
    const data = event.data as { type?: string; components?: StudioComponent[] };
    if (data?.type === 'kf-studio-layout' && Array.isArray(data.components)) {
      studioPreviewComponents.set(data.components.filter((item) => !item.hidden));
    }
  });
  return true;
}

export function studioPreviewStyle(item: StudioComponent): string {
  const style = item.style;
  const opacity = Math.max(0, Math.min(100, style.opacity)) / 100;
  const em = (value: number) => `${Math.round((Math.max(0, value) / STUDIO_BASE_FONT) * 1000) / 1000}em`;
  return [
    `left:${item.x / 100}%`,
    `top:${item.y / 100}%`,
    `width:${item.width / 100}%`,
    `height:${item.height / 100}%`,
    `opacity:${opacity}`,
    `border-radius:${em(style.radius)}`,
    `padding:${em(style.padding)}`,
    `font-size:${em(Math.max(7, style.fontSize))}`,
    `font-weight:${Math.max(100, Math.min(900, style.fontWeight))}`,
    `text-align:${style.textAlign}`,
    `color:${style.foreground}`,
    `background:${style.background}`,
    `border-color:${style.border}`,
    `box-shadow:${style.shadow}`,
  ].join(';');
}
