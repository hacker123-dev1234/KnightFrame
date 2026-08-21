// 界面缩放：Tauri 环境优先原生 webview zoom（浏览器级整体缩放，坐标系统一，
// 鼠标命中 / 拖拽区 / fixed 浮层全部正确跟随，避开 CSS zoom 叠在 <html> 上
// 的命中偏移与 100vh 放大溢出）；仅在浏览器 dev 环境或原生调用失败时退回 CSS zoom。
let sequence = 0;

export async function applyUiScale(scale: number): Promise<void> {
  if (!Number.isFinite(scale) || scale <= 0) return;
  const current = ++sequence;
  const fallback = () => {
    if (current === sequence) document.documentElement.style.setProperty('zoom', String(scale));
  };
  if ('__TAURI_INTERNALS__' in window) {
    try {
      const { getCurrentWebview } = await import('@tauri-apps/api/webview');
      if (current !== sequence) return;
      await getCurrentWebview().setZoom(scale);
    } catch {
      fallback();
    }
  } else {
    fallback();
  }
}
