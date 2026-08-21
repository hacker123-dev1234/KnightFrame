<script lang="ts">
  import { onMount } from 'svelte';
  import { AlignCenter, ArrowDownToLine, ArrowUpToLine, BoxSelect, Braces, Check, ChevronDown, Code2, Copy, Eye, EyeOff, FileText, Grid2X2, GripVertical, Image, Layers3, Link2, ListFilter, Lock, Minus, MousePointerClick, PanelTop, Play, Plus, Redo2, RotateCcw, Send, SlidersHorizontal, TextCursorInput, ToggleLeft, Trash2, Type, Undo2, Unlock, Wrench, X, Zap } from '@lucide/svelte';
  import { bridge } from '../bridge';
  import type { Locale } from '../types';
  import {
    DEFAULT_STUDIO_CONTROLLER,
    DEFAULT_STUDIO_STYLE,
    DEFAULT_STUDIO_STYLE_DSH,
    STUDIO_BASE_FONT,
    STUDIO_CANVAS_SIZE,
    STUDIO_STAGE_HEIGHT,
    STUDIO_STAGE_WIDTH,
    clampCoordinate,
    createStudioAction,
    createStudioComponent,
    normalizeStudioComponent,
    runtimeStudioSlot,
    studioBaseStyle,
    studioDocument,
    studioManifest,
    studioSlotCapability,
    studioSlots,
    type StudioActionType,
    type StudioAskRequest,
    type StudioComponent,
    type StudioComponentType,
    type StudioDocument,
    type StudioEventAction,
    type StudioEventName,
    type StudioExportPreview,
    type StudioTarget,
  } from '../studio';
  import { translate } from '../i18n';

  type StudioView = 'design' | 'code' | 'assistant';
  type CodeFile = 'layout' | 'style' | 'controller' | 'manifest' | 'contribution' | 'client' | 'cordis';
  type LayerRow = { component: StudioComponent; depth: number };
  type StudioCheck = { key: string; status: 'pass' | 'warn' | 'fail' };

  export let locale: Locale;
  export let onClose: () => void;
  export let onAsk: (request: StudioAskRequest) => Promise<{ ok: boolean }>;
  export let onExport: (document: StudioDocument) => Promise<{ ok: boolean; path?: string }>;
  export let onPreview: (document: StudioDocument) => Promise<StudioExportPreview>;
  export let onDshStart: () => Promise<{ available: boolean; running: boolean; url?: string; reason?: string }>;
  export let onDshStop: () => Promise<{ ok: boolean }>;

  let view: StudioView = 'design';
  let target: StudioTarget = 'knightframe';
  let pluginId = 'local.untitled';
  let pluginName = '';
  let components: StudioComponent[] = [];
  let selectedId: string | undefined;
  let undoStack: StudioComponent[][] = [];
  let redoStack: StudioComponent[][] = [];
  let preview = false;
  let requirement = '';
  let sending = false;
  let exporting = false;
  let notice = '';
  let noticeError = false;
  let canvas: HTMLDivElement;
  let activeCode: CodeFile = 'layout';
  let layoutSource = '';
  let styleSource = DEFAULT_STUDIO_STYLE;
  let controllerSource = DEFAULT_STUDIO_CONTROLLER;
  let generated: StudioExportPreview | undefined;
  let generation = 0;
  let dshUrl: string | undefined;
  let dshStarting = false;
  let dshReason = '';
  let zoom = 100;
  let snap = 50;
  let previewData: Record<string, string | boolean> = {};
  let runtimeStyle: HTMLStyleElement | undefined;
  // 拖拽吸附/避让状态：参考线 + 坐标徽章 + 放置避让
  const SNAP_TOLERANCE = 170;
  const AVOID_GAP = 60;
  let dragState: { x: number; y: number; guideV?: number; guideH?: number } | undefined;
  let contextMenu: { x: number; y: number; componentId: string } | undefined;
  let contextInput: { mode: 'url' | 'notify' | 'page'; value: string } | undefined;
  // 固定参考舞台的自适应缩放（ResizeObserver 注入 --stage-fit）：窗口/界面缩放
  // 变化时整体等比缩放，组件几何始终相对 1280×720 舞台，导出永不错位
  let stageFit = 1;
  // 预览模式的 notify 弹出框：居中 dialog（不是顶部横幅）
  let dialog: { message: string } | undefined;

  $: t = (key: string, args?: Record<string, string | number>) => translate(locale, key, args);
  $: selected = components.find((item) => item.id === selectedId);
  $: document = studioDocument(pluginId, pluginName || t('studio.untitled'), target, components, { style: styleSource, controller: controllerSource });
  $: manifestSource = generated?.manifestJson ?? JSON.stringify(studioManifest(document), null, 2);
  $: codeSource = activeCode === 'manifest' ? manifestSource : activeCode === 'contribution' ? generated?.clientContributionJson ?? t('studio.code.loading') : activeCode === 'client' ? generated?.dshClientCode ?? t('studio.code.loading') : generated?.dshDefineArgumentsJson ?? t('studio.code.loading');
  $: slots = studioSlots(target);
  $: selectedSlot = studioSlotCapability(target, selected?.slot ?? 'shell.overlay');
  $: layers = buildLayerRows();
  $: availableParents = components.filter((item) => item.type === 'panel' && item.id !== selectedId && !isDescendant(item.id, selectedId));
  $: if (runtimeStyle) runtimeStyle.textContent = styleSource;
  $: studioChecks = buildStudioChecks();
  $: blockingChecks = studioChecks.filter((check) => check.status === 'fail').length;

  function validLayoutJson(): boolean { try { JSON.parse(layoutSource || JSON.stringify(document)); return true; } catch { return false; } }
  function buildStudioChecks(): StudioCheck[] {
    const idValid = /^[a-z0-9][a-z0-9._-]{2,63}$/i.test(pluginId);
    const unsupported = components.filter((item) => studioSlotCapability(target, item.slot).status === 'unsupported').length;
    return [
      { key: 'studio.check.identity', status: idValid && Boolean((pluginName || '').trim()) ? 'pass' : 'fail' },
      { key: 'studio.check.layout', status: components.length ? 'pass' : 'warn' },
      { key: 'studio.check.slots', status: unsupported ? 'fail' : 'pass' },
      { key: 'studio.check.code', status: validLayoutJson() ? 'pass' : 'fail' },
      { key: 'studio.check.host', status: target === 'knightframe' || dshUrl ? 'pass' : 'warn' },
      { key: 'studio.check.adapter', status: generated ? 'pass' : 'warn' },
    ];
  }

  const palette: { type: StudioComponentType; icon: typeof Plus; key: string }[] = [
    { type: 'button', icon: MousePointerClick, key: 'studio.component.button' },
    { type: 'toggle', icon: ToggleLeft, key: 'studio.component.toggle' },
    { type: 'text', icon: Type, key: 'studio.component.text' },
    { type: 'input', icon: TextCursorInput, key: 'studio.component.input' },
    { type: 'select', icon: ListFilter, key: 'studio.component.select' },
    { type: 'separator', icon: Minus, key: 'studio.component.separator' },
    { type: 'panel', icon: PanelTop, key: 'studio.component.panel' },
    { type: 'container', icon: Layers3, key: 'studio.component.container' },
    { type: 'grid', icon: Grid2X2, key: 'studio.component.grid' },
    { type: 'markdown', icon: FileText, key: 'studio.component.markdown' },
    { type: 'image', icon: Image, key: 'studio.component.image' },
    { type: 'code', icon: Code2, key: 'studio.component.code' },
  ];
  const codeFiles: { id: CodeFile; key: string; editable: boolean }[] = [
    { id: 'layout', key: 'studio.code.layout', editable: true },
    { id: 'style', key: 'studio.code.style', editable: true },
    { id: 'controller', key: 'studio.code.controller', editable: true },
    { id: 'manifest', key: 'studio.code.manifest', editable: false },
    { id: 'contribution', key: 'studio.code.contribution', editable: false },
    { id: 'client', key: 'studio.code.client', editable: false },
    { id: 'cordis', key: 'studio.code.cordis', editable: false },
  ];
  const eventNames: StudioEventName[] = ['click', 'change', 'input', 'focus', 'submit'];
  const actionTypes: StudioActionType[] = ['command', 'setValue', 'toggle', 'notify', 'openUrl', 'openPage'];
  // openPage 白名单：与后端 STUDIO_UI_PAGES 对齐
  const uiPages = ['workspace', 'market', 'browser', 'settings', 'graph', 'studio'];

  // —— 零编程续航：文档落 localStorage，重开工坊不丢作品 ——
  const STORE_KEY = 'knightframe.studio.document.v1';
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let restored = false;

  onMount(() => {
    runtimeStyle = globalThis.document.createElement('style');
    runtimeStyle.dataset.knightframeStudio = 'plugin-preview';
    globalThis.document.head.append(runtimeStyle);
    window.addEventListener('message', handlePreviewMessage);
    try {
      const raw = localStorage.getItem(STORE_KEY);
      if (raw) {
        const saved = JSON.parse(raw) as {
          id?: string; name?: string; target?: StudioTarget;
          components?: StudioComponent[]; style?: string; controller?: string;
        };
        if (Array.isArray(saved.components) && saved.components.length) {
          pluginId = saved.id || pluginId;
          pluginName = saved.name || '';
          target = saved.target === 'dsh' ? 'dsh' : 'knightframe';
          components = saved.components.map(normalizeStudioComponent);
          if (saved.style) styleSource = saved.style;
          if (saved.controller) controllerSource = saved.controller;
        }
      }
    } catch { /* 脏存档直接忽略 */ }
    restored = true;
    // 恢复到 DSH 目标时自动拉起真实宿主
    if (target === 'dsh') void startDshPreview();
    return () => {
      runtimeStyle?.remove();
      window.removeEventListener('message', handlePreviewMessage);
      if (saveTimer) clearTimeout(saveTimer);
    };
  });
  // 舞台自适应：观察画布容器（.studio-canvas-stage），等比 fit 注入 --stage-fit。
  // 组件几何相对固定 1280×720 舞台，界面缩放/窗口变化只整体缩放不错位。
  // canvas 随 {#if view === 'design'} 销毁重建，必须响应式重绑 observer，
  // 否则切视图回来后 observer 还挂在旧 DOM 上，fit 冻结、画布溢出。
  let stageObserver: ResizeObserver | undefined;
  $: if (typeof ResizeObserver !== 'undefined') {
    stageObserver?.disconnect();
    const stage = canvas?.parentElement;
    if (stage) {
      stageObserver = new ResizeObserver((entries) => {
        for (const entry of entries) {
          const { width, height } = entry.contentRect;
          stageFit = width > 0 && height > 0
            ? Math.min(width / STUDIO_STAGE_WIDTH, height / STUDIO_STAGE_HEIGHT)
            : 1;
        }
      });
      stageObserver.observe(stage);
    }
  }
  $: if (restored) persistDocument(pluginId, pluginName, target, components, styleSource, controllerSource);
  function persistDocument(id: string, name: string, nextTarget: StudioTarget, items: StudioComponent[], style: string, controller: string) {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = window.setTimeout(() => {
      try {
        localStorage.setItem(STORE_KEY, JSON.stringify({
          id, name, target: nextTarget, components: items, style, controller,
        }));
      } catch { /* 存储满等异常静默 */ }
    }, 400);
  }

  function clone(items = components): StudioComponent[] { return structuredClone(items); }
  function remember(previous = clone()) { undoStack = [...undoStack.slice(-59), previous]; redoStack = []; }
  function snapValue(value: number) { return snap > 1 ? Math.round(value / snap) * snap : Math.round(value); }
  function showError(cause: unknown) {
    const value = cause as { key?: string; args?: Record<string, string | number> };
    notice = translate(locale, value?.key ?? 'studio.error', value?.args ?? {}); noticeError = true;
  }
  function scheduleGenerated() {
    const id = ++generation; const snapshot = structuredClone(document);
    postPreviewLayout();
    window.setTimeout(async () => {
      try { const result = await onPreview(snapshot); if (id === generation) generated = result; }
      catch (cause) { if (id === generation) showError(cause); }
    }, 120);
  }
  function openView(next: StudioView) {
    if (next === 'code' && view !== 'code') { layoutSource = JSON.stringify(document, null, 2); scheduleGenerated(); }
    view = next;
  }
  function add(type: StudioComponentType, label: string) {
    remember(); const item = createStudioComponent(type, components.length, label, target);
    if (selected?.type === 'panel') { item.parentId = selected.id; item.slot = selected.slot; }
    components = [...components, item]; selectedId = item.id; preview = false; scheduleGenerated();
  }
  // —— 零编程拖放：从组件库按住拖到画布任意位置放下即创建（点击仍在中心添加） ——
  let paletteDrag: { type: StudioComponentType; label: string; ghostX: number; ghostY: number; over: boolean } | undefined;
  function withinCanvas(point: PointerEvent): boolean {
    if (!canvas) return false;
    const bounds = canvas.getBoundingClientRect();
    return point.clientX >= bounds.left && point.clientX <= bounds.right && point.clientY >= bounds.top && point.clientY <= bounds.bottom;
  }
  function beginPaletteDrag(event: PointerEvent, type: StudioComponentType, key: string) {
    if (event.button !== 0) return;
    event.preventDefault();
    const label = t(key);
    paletteDrag = { type, label, ghostX: event.clientX, ghostY: event.clientY, over: false };
    const move = (next: PointerEvent) => {
      if (!paletteDrag) return;
      paletteDrag = { ...paletteDrag, ghostX: next.clientX, ghostY: next.clientY, over: withinCanvas(next) };
    };
    const up = (final: PointerEvent) => {
      window.removeEventListener('pointermove', move);
      const drag = paletteDrag; paletteDrag = undefined;
      if (drag && withinCanvas(final) && canvas) {
        const bounds = canvas.getBoundingClientRect();
        const dropX = snapValue(((final.clientX - bounds.left) / bounds.width) * STUDIO_CANVAS_SIZE);
        const dropY = snapValue(((final.clientY - bounds.top) / bounds.height) * STUDIO_CANVAS_SIZE);
        remember();
        const item = createStudioComponent(drag.type, components.length, drag.label, target);
        item.x = clampCoordinate(dropX - item.width / 2, item.width);
        item.y = clampCoordinate(dropY - item.height / 2, item.height);
        const next = [...components, item];
        components = resolveOverlaps(item, next);
        selectedId = item.id; preview = false; scheduleGenerated();
      }
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up, { once: true });
  }
  // —— 双击画布组件直接改文案（零编程编辑入口） ——
  let editingId: string | undefined;
  let editingDraft = '';
  function startInlineEdit(event: MouseEvent, item: StudioComponent) {
    if (preview) return;
    event.preventDefault(); event.stopPropagation();
    editingId = item.id; editingDraft = item.props.label; selectedId = item.id;
  }
  function commitInlineEdit(id: string) {
    if (!editingId) return;
    const target = components.find((item) => item.id === id);
    editingId = undefined;
    if (!target || target.props.label === editingDraft) return;
    updateSelectedById(id, { props: { ...target.props, label: editingDraft } });
  }
  // —— 起步模板：一键放入一组常用组件，零编程用户从这里开始改 ——
  function applyTemplate(kind: 'toolbar' | 'panel' | 'dashboard') {
    remember();
    const build = (type: StudioComponentType, label: string, x: number, y: number): StudioComponent => {
      const item = createStudioComponent(type, components.length, label, target);
      item.x = clampCoordinate(x, item.width); item.y = clampCoordinate(y, item.height);
      return item;
    };
    let added: StudioComponent[] = [];
    if (kind === 'toolbar') {
      added = [
        build('button', t('studio.component.button'), 620, 8760),
        build('button', t('studio.component.button'), 3400, 8760),
        build('toggle', t('studio.component.toggle'), 6180, 8760),
        build('separator', t('studio.component.separator'), 200, 8600),
      ];
    } else if (kind === 'panel') {
      const panel = build('panel', t('studio.templates.panelTitle'), 3400, 3600);
      added = [
        panel,
        build('text', t('studio.templates.panelText'), 3700, 4500),
        build('input', t('studio.component.input'), 3700, 5400),
        build('button', t('studio.component.button'), 3700, 6600),
      ];
      for (const child of added.slice(1)) child.parentId = panel.id;
    } else {
      const panel = build('panel', t('studio.templates.dashTitle'), 3000, 3200);
      added = [
        panel,
        build('text', t('studio.templates.dashText'), 3300, 4100),
        build('button', t('studio.component.button'), 3300, 5100),
        build('button', t('studio.component.button'), 6200, 5100),
        build('toggle', t('studio.component.toggle'), 3300, 6200),
        build('select', t('studio.component.select'), 6200, 6200),
      ];
      for (const child of added.slice(1)) child.parentId = panel.id;
    }
    const next = [...components, ...added];
    const mover = added[0];
    components = resolveOverlaps(mover, next);
    selectedId = mover.id; preview = false; scheduleGenerated();
  }
  async function startDshPreview() {
    dshStarting = true; dshReason = '';
    try {
      const status = await onDshStart();
      dshUrl = status.running ? status.url : undefined;
      dshReason = status.reason ?? '';
    } catch (cause) { dshUrl = undefined; showError(cause); }
    finally { dshStarting = false; }
  }
  async function selectTarget(next: StudioTarget) {
    if (next === target) {
      // 已在 DSH 目标但宿主没起来：再次点击 = 重试启动
      if (next === 'dsh' && !dshUrl) await startDshPreview();
      return;
    }
    // 样式主题跟随目标：用户没改过默认样式就整套切换（KF 黑白 ↔ DSH 蓝），
    // 自定义过的样式原样保留，不做 surprise 覆盖。
    const previousBase = studioBaseStyle(target);
    if (styleSource === previousBase) styleSource = studioBaseStyle(next);
    target = next; generated = undefined; notice = ''; scheduleGenerated();
    if (next === 'dsh') {
      await startDshPreview();
    } else {
      dshUrl = undefined; dshReason = '';
      await onDshStop().catch(() => undefined);
    }
  }
  function removeSelected() {
    if (!selected) return;
    remember(); const removed = new Set([selected.id]);
    let changed = true;
    while (changed) {
      changed = false;
      for (const item of components) if (item.parentId && removed.has(item.parentId) && !removed.has(item.id)) { removed.add(item.id); changed = true; }
    }
    components = components.filter((item) => !removed.has(item.id)); selectedId = undefined; scheduleGenerated();
  }
  function reset() { if (!components.length) return; remember(); components = []; selectedId = undefined; scheduleGenerated(); }
  function undo() {
    const previous = undoStack[undoStack.length - 1]; if (!previous) return;
    redoStack = [...redoStack, clone()]; components = clone(previous); undoStack = undoStack.slice(0, -1);
    if (!components.some((item) => item.id === selectedId)) selectedId = undefined; scheduleGenerated();
  }
  function redo() {
    const next = redoStack[redoStack.length - 1]; if (!next) return;
    undoStack = [...undoStack, clone()]; components = clone(next); redoStack = redoStack.slice(0, -1); scheduleGenerated();
  }
  function patchComponent(id: string, patch: Partial<StudioComponent>) {
    const before = clone(); components = components.map((item) => item.id === id ? { ...item, ...patch } : item); remember(before); scheduleGenerated();
  }
  function updateSelected(patch: Partial<StudioComponent>, props?: Partial<StudioComponent['props']>) {
    if (!selected) return; const before = clone();
    components = components.map((item) => item.id === selected.id ? { ...item, ...patch, props: { ...item.props, ...props } } : item);
    remember(before); scheduleGenerated();
  }
  function updateStyle(patch: Partial<StudioComponent['style']>) { if (selected) updateSelected({ style: { ...selected.style, ...patch } }); }
  // —— 即时样式滑条：拖动过程只改画布（并实时推宿主覆盖层），松手才记一次撤销 ——
  let styleDragBefore: StudioComponent[] | undefined;
  function styleInput(patch: Partial<StudioComponent['style']>) {
    if (!selected) return;
    if (!styleDragBefore) styleDragBefore = clone();
    components = components.map((item) => item.id === selected.id ? { ...item, style: { ...item.style, ...patch } } : item);
    postPreviewLayout();
  }
  // 宽高滑条：与样式滑条共用"即时改、松手记撤销"的节奏
  function sizeInput(patch: Partial<Pick<StudioComponent, 'width' | 'height'>>) {
    if (!selected) return;
    if (!styleDragBefore) styleDragBefore = clone();
    components = components.map((item) => item.id === selected.id ? { ...item, ...patch } : item);
    postPreviewLayout();
  }
  function styleCommit() {
    if (styleDragBefore) { remember(styleDragBefore); styleDragBefore = undefined; scheduleGenerated(); postPreviewLayout(); }
  }
  // —— 尺寸句柄：四角拖拽直接改宽高（西/北角同步移动 x/y） ——
  function beginResize(event: PointerEvent, item: StudioComponent, dir: 'nw' | 'ne' | 'sw' | 'se') {
    if (preview || item.locked || event.button !== 0) return;
    event.preventDefault(); event.stopPropagation();
    const before = clone(); const bounds = canvas.getBoundingClientRect(); const startX = event.clientX; const startY = event.clientY;
    const initial = { x: item.x, y: item.y, width: item.width, height: item.height };
    const move = (next: PointerEvent) => {
      const dx = ((next.clientX - startX) / bounds.width) * STUDIO_CANVAS_SIZE;
      const dy = ((next.clientY - startY) / bounds.height) * STUDIO_CANVAS_SIZE;
      let { x, y, width, height } = initial;
      if (dir === 'se' || dir === 'ne') width = initial.width + dx;
      if (dir === 'sw' || dir === 'nw') { width = initial.width - dx; x = initial.x + dx; }
      if (dir === 'se' || dir === 'sw') height = initial.height + dy;
      if (dir === 'ne' || dir === 'nw') { height = initial.height - dy; y = initial.y + dy; }
      width = Math.max(120, Math.min(STUDIO_CANVAS_SIZE, snapValue(width)));
      height = Math.max(80, Math.min(STUDIO_CANVAS_SIZE, snapValue(height)));
      if (x < 0) { width += x; x = 0; }
      if (y < 0) { height += y; y = 0; }
      components = components.map((candidate) => candidate.id === item.id
        ? { ...candidate, x: clampCoordinate(x, width), y: clampCoordinate(y, height), width, height }
        : candidate);
      postPreviewLayout();
    };
    const end = () => {
      window.removeEventListener('pointermove', move);
      remember(before); scheduleGenerated(); postPreviewLayout();
    };
    window.addEventListener('pointermove', move); window.addEventListener('pointerup', end, { once: true });
  }
  // —— KF 原生互动预览：把布局推给宿主 iframe（真实 KnightFrame 界面上的覆盖层） ——
  function postPreviewLayout() {
    if (target !== 'knightframe') return;
    const frame = canvas?.querySelector('iframe.studio-host-frame') as HTMLIFrameElement | null;
    frame?.contentWindow?.postMessage({ type: 'kf-studio-layout', components: components.filter((item) => !item.hidden) }, '*');
  }
  function handlePreviewMessage(event: MessageEvent) {
    const data = event.data as { type?: string; url?: string; page?: string };
    if (data?.type === 'kf-studio-open-url' && data.url) {
      notice = translate(locale, 'studio.action.urlPreview', { url: data.url }); noticeError = false;
      void bridge.browserCommand('open', data.url).catch(() => undefined);
    }
    if (data?.type === 'kf-studio-open-page' && data.page && uiPages.includes(data.page)) {
      notice = translate(locale, 'studio.action.pagePreview', { page: data.page }); noticeError = false;
      void bridge.relayUiPage(data.page).catch(() => undefined);
    }
  }
  function updateBinding(patch: Partial<StudioComponent['binding']>) { if (selected) updateSelected({ binding: { ...selected.binding, ...patch } }); }
  function duplicateSelected() {
    if (!selected) return; remember(); const copy = structuredClone(selected); copy.id = `${copy.type}-${crypto.randomUUID()}`;
    copy.actions = copy.actions.map((action) => ({ ...action, id: crypto.randomUUID() }));
    copy.x = clampCoordinate(copy.x + 240, copy.width); copy.y = clampCoordinate(copy.y + 240, copy.height);
    components = [...components, copy]; selectedId = copy.id; scheduleGenerated();
  }
  function reorder(direction: 'front' | 'back') {
    if (!selected) return; remember(); components = components.filter((item) => item.id !== selected.id);
    components = direction === 'front' ? [...components, selected] : [selected, ...components]; scheduleGenerated();
  }
  function centerSelected() {
    if (!selected) return;
    updateSelected({ x: Math.round((STUDIO_CANVAS_SIZE - selected.width) / 2), y: Math.round((STUDIO_CANVAS_SIZE - selected.height) / 2) });
  }
  // —— 拖拽吸附：与画布边缘/中心、其他组件的边/中线对齐，画参考线 + 坐标徽章 ——
  function snapEdges(value: number, size: number, candidates: number[]): { delta: number; guide: number; best: number } {
    let delta = 0; let guide = 0; let best = SNAP_TOLERANCE;
    for (const candidate of candidates) {
      const edges = [value, value + size / 2, value + size];
      for (let index = 0; index < edges.length; index += 1) {
        const distance = Math.abs(candidate - edges[index]);
        if (distance < best) { best = distance; delta = candidate - edges[index]; guide = candidate; }
      }
    }
    return { delta, guide, best };
  }
  function snapCandidates(excludeId: string) {
    const xs = [0, STUDIO_CANVAS_SIZE / 2, STUDIO_CANVAS_SIZE];
    const ys = [0, STUDIO_CANVAS_SIZE / 2, STUDIO_CANVAS_SIZE];
    for (const other of components) {
      if (other.id === excludeId) continue;
      xs.push(other.x, other.x + other.width / 2, other.x + other.width);
      ys.push(other.y, other.y + other.height / 2, other.y + other.height);
    }
    return { xs, ys };
  }
  function beginDrag(event: PointerEvent, item: StudioComponent) {
    if (preview || item.locked || event.button !== 0) return; event.preventDefault(); selectedId = item.id;
    const before = clone(); const bounds = canvas.getBoundingClientRect(); const startX = event.clientX; const startY = event.clientY;
    const initialX = item.x; const initialY = item.y; let moved = false;
    const move = (next: PointerEvent) => {
      const dx = ((next.clientX - startX) / bounds.width) * STUDIO_CANVAS_SIZE; const dy = ((next.clientY - startY) / bounds.height) * STUDIO_CANVAS_SIZE;
      moved ||= Math.abs(dx) > 2 || Math.abs(dy) > 2;
      const rawX = clampCoordinate(snapValue(initialX + dx), item.width);
      const rawY = clampCoordinate(snapValue(initialY + dy), item.height);
      const { xs, ys } = snapCandidates(item.id);
      const snapX = snapEdges(rawX, item.width, xs);
      const snapY = snapEdges(rawY, item.height, ys);
      const x = clampCoordinate(rawX + snapX.delta, item.width);
      const y = clampCoordinate(rawY + snapY.delta, item.height);
      dragState = { x, y, guideV: snapX.best < SNAP_TOLERANCE ? snapX.guide : undefined, guideH: snapY.best < SNAP_TOLERANCE ? snapY.guide : undefined };
      components = components.map((candidate) => candidate.id === item.id ? { ...candidate, x, y } : candidate);
      // 宿主覆盖层实时跟随，杜绝"编辑层在动、宿主层残留"的叠影
      postPreviewLayout();
    };
    const end = () => {
      window.removeEventListener('pointermove', move);
      dragState = undefined;
      if (moved) { components = resolveOverlaps(item, components); remember(before); scheduleGenerated(); }
    };
    window.addEventListener('pointermove', move); window.addEventListener('pointerup', end, { once: true });
  }
  // —— 放置避让：把与新位置重叠的无关节点沿最小向量推离，为放置者腾出准确位置 ——
  function related(idA: string, idB: string): boolean {
    return isDescendant(idA, idB) || isDescendant(idB, idA) || idA === idB;
  }
  function overlaps(a: { x: number; y: number; width: number; height: number }, b: { x: number; y: number; width: number; height: number }): boolean {
    return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
  }
  function resolveOverlaps(mover: StudioComponent, list: StudioComponent[]): StudioComponent[] {
    const movingIds = new Set([mover.id]);
    for (let iteration = 0; iteration < 6; iteration += 1) {
      let changed = false;
      for (const item of list) {
        if (movingIds.has(item.id) || item.locked || item.hidden) continue;
        const current = list.find((candidate) => candidate.id === item.id);
        if (!current || related(mover.id, item.id) || !overlaps(mover, current)) continue;
        const overlapX = Math.min(mover.x + mover.width, current.x + current.width) - Math.max(mover.x, current.x);
        const overlapY = Math.min(mover.y + mover.height, current.y + current.height) - Math.max(mover.y, current.y);
        const dirX = current.x + current.width / 2 >= mover.x + mover.width / 2 ? 1 : -1;
        const dirY = current.y + current.height / 2 >= mover.y + mover.height / 2 ? 1 : -1;
        const shiftX = dirX * (overlapX + AVOID_GAP);
        const shiftY = dirY * (overlapY + AVOID_GAP);
        const push = Math.abs(shiftX) <= Math.abs(shiftY)
          ? { x: shiftX, y: dirY * AVOID_GAP * 0.4 }
          : { x: dirX * AVOID_GAP * 0.4, y: shiftY };
        list = list.map((candidate) => candidate.id === item.id
          ? { ...candidate, x: clampCoordinate(candidate.x + push.x, candidate.width), y: clampCoordinate(candidate.y + push.y, candidate.height) }
          : candidate);
        changed = true;
      }
      // 连锁：被推开的组件若撞上其他无关组件，继续让位（同轮收敛）
      for (const item of list) {
        if (movingIds.has(item.id) || item.locked || item.hidden) continue;
        for (const other of list) {
          if (other.id === item.id || movingIds.has(other.id) || other.locked) continue;
          if (related(item.id, other.id) || !overlaps(item, other)) continue;
          const overlapX = Math.min(item.x + item.width, other.x + other.width) - Math.max(item.x, other.x);
          const overlapY = Math.min(item.y + item.height, other.y + other.height) - Math.max(item.y, other.y);
          const dir = Math.abs(overlapX) <= Math.abs(overlapY)
            ? { x: Math.sign(other.x + other.width / 2 - (item.x + item.width / 2)) * (overlapX + AVOID_GAP), y: 0 }
            : { x: 0, y: Math.sign(other.y + other.height / 2 - (item.y + item.height / 2)) * (overlapY + AVOID_GAP) };
          list = list.map((candidate) => candidate.id === other.id
            ? { ...candidate, x: clampCoordinate(candidate.x + dir.x, candidate.width), y: clampCoordinate(candidate.y + dir.y, candidate.height) }
            : candidate);
          changed = true;
        }
      }
      if (!changed) break;
    }
    return list;
  }
  // —— 右键菜单：设置点击行为（加载页面 / 弹出提示框）与常用操作 ——
  function openContextMenu(event: MouseEvent, item: StudioComponent) {
    event.preventDefault(); event.stopPropagation();
    selectedId = item.id; contextInput = undefined;
    contextMenu = { x: event.clientX, y: event.clientY, componentId: item.id };
  }
  function closeContextMenu() { contextMenu = undefined; contextInput = undefined; }
  function contextTarget(): StudioComponent | undefined {
    return contextMenu ? components.find((item) => item.id === contextMenu?.componentId) : undefined;
  }
  function beginContextInput(mode: 'url' | 'notify' | 'page') {
    const item = contextTarget(); if (!item) return;
    const matchAction = mode === 'url' ? 'openUrl' : mode === 'page' ? 'openPage' : 'notify';
    const existing = item.actions.find((action) => action.event === 'click' && action.action === matchAction);
    contextInput = { mode, value: existing?.value ?? (mode === 'page' ? 'market' : '') };
  }
  function applyContextInput() {
    const item = contextTarget();
    if (!item || !contextInput) return;
    const mode = contextInput.mode;
    const value = contextInput.value.trim();
    closeContextMenu();
    if (!value) return;
    if (mode === 'page' && !uiPages.includes(value)) {
      notice = translate(locale, 'error.plugin_ui_page', { page: value }); noticeError = true;
      return;
    }
    const action = mode === 'url' ? 'openUrl' : mode === 'page' ? 'openPage' : 'notify';
    const cleaned = item.actions.filter((candidate) => !(candidate.event === 'click' && (candidate.action === 'openUrl' || candidate.action === 'openPage' || candidate.action === 'notify')));
    updateSelectedById(item.id, { actions: [...cleaned, { id: crypto.randomUUID(), event: 'click', action, value }] });
    notice = translate(locale, mode === 'url' ? 'studio.context.urlApplied' : mode === 'page' ? 'studio.context.pageApplied' : 'studio.context.notifyApplied'); noticeError = false;
  }
  function clearContextBehavior() {
    const item = contextTarget(); if (!item) return;
    closeContextMenu();
    updateSelectedById(item.id, { actions: item.actions.filter((candidate) => !(candidate.event === 'click' && (candidate.action === 'openUrl' || candidate.action === 'openPage' || candidate.action === 'notify'))) });
  }
  function updateSelectedById(id: string, patch: Partial<StudioComponent>) {
    const before = clone();
    components = components.map((item) => item.id === id ? { ...item, ...patch } : item);
    remember(before); scheduleGenerated();
  }
  function initFocus(node: HTMLElement) { window.setTimeout(() => node.focus(), 30); }
  function addAction() {
    if (!selected) return; updateSelected({ actions: [...selected.actions, createStudioAction()] });
  }
  function updateAction(id: string, patch: Partial<StudioEventAction>) {
    if (!selected) return; updateSelected({ actions: selected.actions.map((item) => item.id === id ? { ...item, ...patch } : item) });
  }
  function removeAction(id: string) { if (selected) updateSelected({ actions: selected.actions.filter((item) => item.id !== id) }); }
  function applyCodeSource() {
    if (activeCode === 'style' || activeCode === 'controller') {
      notice = t('studio.code.applied'); noticeError = false; scheduleGenerated(); return;
    }
    if (activeCode !== 'layout') return;
    try {
      const next = JSON.parse(layoutSource) as StudioDocument; const validTypes = new Set(palette.map((item) => item.type));
      if (!next || next.protocolVersion !== 'knightframe.plugin.v1' || !Array.isArray(next.ui) || next.ui.some((item) => !item?.id || !validTypes.has(item.type))) throw new Error();
      pluginId = String(next.id || 'local.untitled'); pluginName = String(next.name || t('studio.untitled'));
      target = next.target === 'dsh' ? 'dsh' : 'knightframe'; components = next.ui.map(normalizeStudioComponent); selectedId = undefined;
      styleSource = next.sources?.style ?? styleSource; controllerSource = next.sources?.controller ?? controllerSource;
      notice = t('studio.code.applied'); noticeError = false; layoutSource = JSON.stringify({ ...next, ui: components, sources: { style: styleSource, controller: controllerSource } }, null, 2); scheduleGenerated();
    } catch { notice = t('studio.code.invalid'); noticeError = true; }
  }
  function buildLayerRows(): LayerRow[] {
    const rows: LayerRow[] = []; const seen = new Set<string>();
    const visit = (parentId: string | undefined, depth: number) => {
      for (const component of components) if (component.parentId === parentId && !seen.has(component.id)) {
        seen.add(component.id); rows.push({ component, depth }); visit(component.id, depth + 1);
      }
    };
    visit(undefined, 0);
    for (const component of components) if (!seen.has(component.id)) rows.push({ component, depth: 0 });
    return rows;
  }
  function isDescendant(candidateId: string, ancestorId?: string): boolean {
    if (!ancestorId) return false;
    let current = components.find((item) => item.id === candidateId);
    const seen = new Set<string>();
    while (current?.parentId && !seen.has(current.id)) {
      if (current.parentId === ancestorId) return true;
      seen.add(current.id); current = components.find((item) => item.id === current?.parentId);
    }
    return false;
  }
  function componentStyle(item: StudioComponent): string {
    const style = item.style;
    // 尺寸类样式走 em：随画布容器基准字号（0.9375cqw ≈ 12px@1280 参考舞台）缩放，
    // 画布大小变化时控件比例、字号、留白与导出后的宿主舞台完全一致。
    const em = (value: number) => `${Math.round((Math.max(0, value) / STUDIO_BASE_FONT) * 1000) / 1000}em`;
    return `left:${item.x / 100}%;top:${item.y / 100}%;width:${item.width / 100}%;height:${item.height / 100}%;--node-opacity:${Math.max(0, Math.min(100, style.opacity)) / 100};--node-radius:${em(style.radius)};--node-padding:${em(style.padding)};--node-font-size:${em(Math.max(7, style.fontSize))};--node-font-weight:${Math.max(100, Math.min(900, style.fontWeight))};--node-align:${style.textAlign};--node-fg:${style.foreground};--node-bg:${style.background};--node-border:${style.border};--node-shadow:${style.shadow};`;
  }
  function previewValue(item: StudioComponent): string | boolean {
    const key = item.binding.key.trim();
    return key && key in previewData ? previewData[key] : item.binding.defaultValue;
  }
  function firePreviewAction(item: StudioComponent, event: StudioEventName, value: string | boolean = '') {
    if (!preview) return;
    const key = item.binding.key.trim();
    if (key && item.binding.mode !== 'read') previewData = { ...previewData, [key]: value };
    for (const action of item.actions.filter((candidate) => candidate.event === event)) {
      if (action.action === 'setValue' && key) previewData = { ...previewData, [key]: action.value };
      if (action.action === 'toggle' && key) previewData = { ...previewData, [key]: !Boolean(previewData[key]) };
      // notify = 真正的居中 dialog（带遮罩与关闭按钮），不是顶部横幅
      if (action.action === 'notify') { dialog = { message: action.value || t('studio.action.preview') }; }
      if (action.action === 'command') { notice = action.value || t('studio.action.preview'); noticeError = false; }
      if (action.action === 'openUrl') { notice = translate(locale, 'studio.action.urlPreview', { url: action.value }); noticeError = false; }
      // 真实驱动主窗口原生 UI：让插件按钮切页面，预览即所见即所得
      if (action.action === 'openPage' && uiPages.includes(action.value.trim())) {
        notice = translate(locale, 'studio.action.pagePreview', { page: action.value.trim() }); noticeError = false;
        void bridge.relayUiPage(action.value.trim()).catch(() => undefined);
      }
    }
  }
  // —— 工坊/预览双模式：预览=宿主真实可交互（KF 组件交给宿主覆盖层，DSH 组件层穿透）——
  function togglePreview() {
    preview = !preview;
    if (preview) {
      selectedId = undefined; editingId = undefined; closeContextMenu();
      postPreviewLayout(); // 切换瞬间保证宿主覆盖层拿到最新布局
    }
  }
  function handleKeydown(event: KeyboardEvent) {
    // dialog 打开时 Escape 优先关闭弹窗
    if (dialog) { if (event.key === 'Escape') { event.preventDefault(); dialog = undefined; } return; }
    if (view !== 'design' || preview) return;
    const element = event.target as HTMLElement;
    if (['INPUT', 'TEXTAREA', 'SELECT'].includes(element.tagName) || element.isContentEditable) return;
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'z') { event.preventDefault(); event.shiftKey ? redo() : undo(); return; }
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'y') { event.preventDefault(); redo(); return; }
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'd') { event.preventDefault(); duplicateSelected(); return; }
    if (event.key === 'Escape') { selectedId = undefined; return; }
    if ((event.key === 'Delete' || event.key === 'Backspace') && selected) { event.preventDefault(); removeSelected(); return; }
    if (!selected || selected.locked || !event.key.startsWith('Arrow')) return;
    event.preventDefault(); const distance = event.shiftKey ? snap * 5 : snap;
    const x = selected.x + (event.key === 'ArrowLeft' ? -distance : event.key === 'ArrowRight' ? distance : 0);
    const y = selected.y + (event.key === 'ArrowUp' ? -distance : event.key === 'ArrowDown' ? distance : 0);
    updateSelected({ x: clampCoordinate(x, selected.width), y: clampCoordinate(y, selected.height) });
  }
  async function sendRequest() {
    if (!requirement.trim() || sending) return; sending = true;
    try {
      const selectedSummary = selected ? `${selected.type} · ${selected.id} · ${selected.slot} · ${selected.x},${selected.y},${selected.width},${selected.height}` : t('studio.ask.none');
      const content = translate(locale, 'studio.ask.prompt', { target: target === 'dsh' ? t('studio.target.dsh') : 'KnightFrame', requirement: requirement.trim(), name: document.name, id: document.id, count: components.length, selected: selectedSummary });
      const result = await onAsk({ target, selected: selected ? structuredClone(selected) : undefined, layout: document, requirement: requirement.trim(), content });
      if (result.ok) { requirement = ''; notice = t('studio.ask.success'); noticeError = false; }
    } catch (cause) { showError(cause); } finally { sending = false; }
  }
  async function exportDocument() {
    if (exporting) return; exporting = true;
    try { const result = await onExport(document); notice = result.ok ? translate(locale, 'studio.export.success', { path: result.path ?? '' }) : t('studio.export.cancelled'); noticeError = false; }
    catch (cause) { showError(cause); } finally { exporting = false; }
  }
</script>

<svelte:window on:keydown={handleKeydown} on:pointerdown={(event) => { if (contextMenu && !(event.target as HTMLElement).closest('.studio-context-menu')) closeContextMenu(); }} on:blur={closeContextMenu} />

<section class="studio-shell">
  <header class="studio-topbar" data-tauri-drag-region>
    <div class="studio-brand" data-tauri-drag-region><BoxSelect size={18} /><strong>{t('studio.title')}</strong><span class="studio-beta">BETA</span></div>
    <nav class="studio-view-tabs" aria-label={t('studio.views')}>
      <button class:active={view === 'design'} type="button" on:click={() => openView('design')}><BoxSelect size={14} />{t('studio.view.design')}</button>
      <button class:active={view === 'code'} type="button" on:click={() => openView('code')}><Code2 size={14} />{t('studio.view.code')}</button>
      <button class:active={view === 'assistant'} type="button" on:click={() => openView('assistant')}><Send size={14} />{t('studio.view.assistant')}</button>
    </nav>
    <div class="studio-actions">
      {#if view === 'design'}
        <button type="button" disabled={!undoStack.length} on:click={undo} title={t('studio.undo')}><Undo2 size={16} /></button>
        <button type="button" disabled={!redoStack.length} on:click={redo} title={t('studio.redo')}><Redo2 size={16} /></button>
        <button type="button" disabled={!selected} on:click={removeSelected} title={t('studio.delete')}><Trash2 size={16} /></button>
        <button type="button" disabled={!components.length} on:click={reset} title={t('studio.reset')}><RotateCcw size={16} /></button>
        <button class:active={preview} type="button" on:click={togglePreview} title={t('studio.preview')}>{#if preview}<EyeOff size={16} />{:else}<Eye size={16} />{/if}</button>
      {/if}
      <button class="studio-export" type="button" disabled={exporting || blockingChecks > 0} title={blockingChecks ? t('studio.check.blocked') : t('studio.export')} on:click={exportDocument}><ChevronDown size={15} />{t('studio.export')}</button>
      <button type="button" on:click={onClose} title={t('app.close')}><X size={17} /></button>
    </div>
  </header>
  <div class="studio-flow" aria-label={t('studio.flow')}>
    <button class:active={view === 'design'} type="button" on:click={() => openView('design')}><small>01</small><span>{t('studio.flow.target')}</span></button>
    <i></i><button class:active={view === 'design'} type="button" on:click={() => openView('design')}><small>02</small><span>{t('studio.flow.compose')}</span></button>
    <i></i><button class:active={view === 'code'} type="button" on:click={() => openView('code')}><small>03</small><span>{t('studio.flow.code')}</span></button>
    <i></i><button class:active={view === 'assistant'} type="button" on:click={() => openView('assistant')}><small>04</small><span>{t('studio.flow.assist')}</span></button>
    <div class="studio-readiness">{#each studioChecks as check}<span class={check.status} title={t(check.key)}><Check size={10} />{t(check.key)}</span>{/each}</div>
  </div>

  {#if view === 'design'}
    <div class="studio-workspace">
      <aside class="studio-palette">
        <section class="studio-library">
          <div class="studio-panel-heading"><span>{t('studio.palette')}</span><small>{palette.length}</small></div>
          <div class="studio-palette-list">{#each palette as item}<button type="button" on:pointerdown={(event) => beginPaletteDrag(event, item.type, item.key)} on:click={() => add(item.type, t(item.key))} title={t(item.key)}><svelte:component this={item.icon} size={16} /><span>{t(item.key)}</span><Plus size={12} /></button>{/each}</div>
        </section>
        <section class="studio-templates">
          <div class="studio-panel-heading"><span>{t('studio.templates.title')}</span></div>
          <div class="studio-template-list">
            <button type="button" on:click={() => applyTemplate('toolbar')}><Zap size={13} /><span>{t('studio.templates.toolbar')}</span></button>
            <button type="button" on:click={() => applyTemplate('panel')}><PanelTop size={13} /><span>{t('studio.templates.panel')}</span></button>
            <button type="button" on:click={() => applyTemplate('dashboard')}><Grid2X2 size={13} /><span>{t('studio.templates.dashboard')}</span></button>
          </div>
          <p class="studio-template-hint">{t('studio.templates.hint')}</p>
        </section>
        <section class="studio-layers">
          <div class="studio-panel-heading"><span>{t('studio.layers')}</span><small>{components.length}</small></div>
          <div class="studio-layer-list" role="tree" aria-label={t('studio.layers')}>
            {#if !layers.length}<p>{t('studio.layers.empty')}</p>{/if}
            {#each layers as row (row.component.id)}
              <div class:selected={row.component.id === selectedId} class="studio-layer-row" style={`--layer-depth:${row.depth}`} role="treeitem" aria-selected={row.component.id === selectedId}>
                <button class="studio-layer-main" type="button" on:click={() => selectedId = row.component.id} title={row.component.id}><GripVertical size={12} /><span>{row.component.props.label || row.component.type}</span><small>{row.component.type}</small></button>
                <button type="button" on:click={() => patchComponent(row.component.id, { hidden: !row.component.hidden })} aria-label={row.component.hidden ? t('studio.layer.show') : t('studio.layer.hide')} title={row.component.hidden ? t('studio.layer.show') : t('studio.layer.hide')}>{#if row.component.hidden}<EyeOff size={12} />{:else}<Eye size={12} />{/if}</button>
                <button type="button" on:click={() => patchComponent(row.component.id, { locked: !row.component.locked })} aria-label={row.component.locked ? t('studio.layer.unlock') : t('studio.layer.lock')} title={row.component.locked ? t('studio.layer.unlock') : t('studio.layer.lock')}>{#if row.component.locked}<Lock size={12} />{:else}<Unlock size={12} />{/if}</button>
              </div>
            {/each}
          </div>
        </section>
        <div class="studio-target-block"><span>{t('studio.target')}</span><div class="studio-target"><button class:active={target === 'knightframe'} type="button" on:click={() => selectTarget('knightframe')}>{t('studio.target.knightframe')}</button><button class:active={target === 'dsh'} type="button" on:click={() => selectTarget('dsh')}>{t('studio.target.dsh')}</button></div></div>
      </aside>
      <main class:preview class:dsh={target === 'dsh'} class="studio-canvas-stage">
        <div class="studio-host-label"><span>{target === 'dsh' ? t('studio.target.dsh').toUpperCase() : 'KNIGHTFRAME'}</span><small>{target === 'dsh' && !dshUrl ? t('studio.host.degraded') : t('studio.host.live')}</small></div>
        <div class="studio-canvas-tools" aria-label={t('studio.canvas.controls')}><div class="studio-mode-toggle" role="group" aria-label={t('studio.mode.label')}><button class:active={!preview} type="button" on:click={() => preview && togglePreview()} title={t('studio.mode.workshop')}><Wrench size={12} /><span>{t('studio.mode.workshop')}</span></button><button class:active={preview} type="button" on:click={() => !preview && togglePreview()} title={t('studio.mode.preview')}><Play size={12} /><span>{t('studio.mode.preview')}</span></button></div><label>{t('studio.snap')}<select bind:value={snap}><option value={1}>1</option><option value={10}>10</option><option value={50}>50</option><option value={100}>100</option></select></label><button type="button" on:click={() => zoom = Math.max(75, zoom - 25)} aria-label={t('studio.zoom.out')}><Minus size={13} /></button><output>{zoom}%</output><button type="button" on:click={() => zoom = Math.min(125, zoom + 25)} aria-label={t('studio.zoom.in')}><Plus size={13} /></button></div>
        <div class="studio-canvas" bind:this={canvas} role="group" aria-label={t('studio.canvas')} style={`--stage-zoom:${zoom / 100}; --stage-fit:${stageFit}`} on:pointerdown={(event) => event.target === canvas && (selectedId = undefined)}>
          {#if target === 'knightframe'}<iframe class="studio-host-frame" src="./index.html?studioPreview=1" title={t('studio.target.knightframe')} on:load={postPreviewLayout}></iframe>
          {:else if dshUrl}<iframe class="studio-host-frame" src={dshUrl} title={t('studio.target.dsh')}></iframe>
          {:else}<div class="studio-host-unavailable"><Braces size={30} /><strong>{dshStarting ? t('studio.dsh.starting') : t('studio.dsh.unavailable')}</strong>{#if dshReason.startsWith('dsh-exited:')}<p>{t('studio.dsh.exited')}</p><code class="studio-dsh-diag">{dshReason.slice('dsh-exited:'.length)}</code>{:else}<p>{dshReason === 'dsh-start-timeout' ? t('studio.dsh.startFailed') : t('studio.dsh.buildRequired')}</p>{/if}</div>{/if}
          <div class="studio-node-layer kf-plugin-surface">{#each components as item (item.id)}
            {#if !item.hidden}<div class:selected={item.id === selectedId} class:locked={preview || item.locked} class={`studio-node studio-node-${item.type} kf-plugin-component ${item.style.className}`} style={componentStyle(item)} role="presentation" title={preview ? undefined : t('studio.canvas.editHint')} on:pointerdown={(event) => beginDrag(event, item)} on:contextmenu={(event) => openContextMenu(event, item)} on:dblclick={(event) => startInlineEdit(event, item)}>
              {#if item.type === 'button'}<button type="button" disabled={item.props.disabled} on:click={() => firePreviewAction(item, 'click', true)}>{item.props.label}</button>
              {:else if item.type === 'toggle'}<button class="studio-node-toggle" type="button" disabled={item.props.disabled} aria-pressed={Boolean(previewValue(item))} on:click={() => firePreviewAction(item, 'change', !Boolean(previewValue(item)))}><span>{item.props.label}</span><i></i></button>
              {:else if item.type === 'text'}<p>{item.props.label}</p>
              {:else if item.type === 'input'}<input value={String(previewValue(item) || '')} placeholder={item.props.label} disabled={item.props.disabled} readonly={item.binding.mode === 'read'} on:input={(event) => firePreviewAction(item, 'input', event.currentTarget.value)} on:focus={() => firePreviewAction(item, 'focus')} />
              {:else if item.type === 'select'}<select class="studio-fake-select" disabled={item.props.disabled} value={String(previewValue(item) || '')} on:change={(event) => firePreviewAction(item, 'change', event.currentTarget.value)}>{#each item.props.options ?? [] as option, index}<option value={`option-${index + 1}`}>{option}</option>{/each}</select>
              {:else if item.type === 'separator'}<hr />
              {:else}<section><strong>{item.props.label}</strong></section>{/if}
              {#if editingId === item.id}
                <input class="studio-inline-edit" bind:value={editingDraft} use:initFocus on:blur={() => commitInlineEdit(item.id)} on:pointerdown|stopPropagation on:click|stopPropagation on:dblclick|stopPropagation on:keydown={(event) => { event.stopPropagation(); if (event.key === 'Enter') commitInlineEdit(item.id); if (event.key === 'Escape') editingId = undefined; }} aria-label={t('studio.property.label')} />
              {/if}
              {#if item.id === selectedId && !preview && !item.locked}
                {#each ['nw', 'ne', 'sw', 'se'] as dir (dir)}
                  <span class="studio-resize-handle" role="button" tabindex={-1} aria-label={t('studio.resize', { dir })} data-dir={dir} on:pointerdown={(event) => beginResize(event, item, dir as 'nw' | 'ne' | 'sw' | 'se')}></span>
                {/each}
              {/if}
            </div>{/if}
          {/each}</div>
          {#if !components.length && !preview}
            <div class="studio-empty-hint"><MousePointerClick size={26} /><strong>{t('studio.empty.title')}</strong><p>{t('studio.empty.hint')}</p></div>
          {/if}
          {#if dragState}
            {#if dragState.guideV !== undefined}<div class="studio-guide vertical" style={`left:${dragState.guideV / 100}%`}></div>{/if}
            {#if dragState.guideH !== undefined}<div class="studio-guide horizontal" style={`top:${dragState.guideH / 100}%`}></div>{/if}
            <div class="studio-drag-badge" style={`left:${dragState.x / 100}%;top:${dragState.y / 100}%`}>{Math.round(dragState.x)}, {Math.round(dragState.y)}</div>
          {/if}
        </div>
        <div class:degraded={target === 'dsh' && !dshUrl} class="studio-capability"><Zap size={12} /><span>{target === 'dsh' ? t(dshUrl ? 'studio.capability.dshLive' : 'studio.capability.dshAdapted') : t('studio.capability.knightframe')}</span></div>
        <div class="studio-schematic-badge" title={t('studio.canvas.schematicHint')}>{t('studio.canvas.schematic')}</div>
      </main>
      <aside class="studio-inspector"><div class="studio-panel-heading"><span>{t('studio.inspector')}</span><SlidersHorizontal size={14} /></div>
        {#if selected}
          <div class="studio-inspector-toolbar"><button type="button" on:click={duplicateSelected} title={t('studio.duplicate')}><Copy size={14} /></button><button type="button" on:click={centerSelected} title={t('studio.center')}><AlignCenter size={14} /></button><button type="button" on:click={() => reorder('front')} title={t('studio.front')}><ArrowUpToLine size={14} /></button><button type="button" on:click={() => reorder('back')} title={t('studio.back')}><ArrowDownToLine size={14} /></button></div>
          <div class="studio-inspector-form">
            <details open><summary>{t('studio.section.content')}</summary><div class="studio-fieldset"><label><span>{t('studio.property.label')}</span><input value={selected.props.label} on:change={(event) => updateSelected({}, { label: event.currentTarget.value })} /></label><label><span>{t('studio.property.slot')}</span><input list="studio-slot-options" value={selected.slot} placeholder={t('studio.slot.search')} on:change={(event) => updateSelected({ slot: event.currentTarget.value.trim() || 'shell.overlay' })} /><datalist id="studio-slot-options">{#each slots as slot}<option value={slot.id}>{slot.status} · {slot.kind} · {slot.scope}</option>{/each}</datalist><small class:warning={selectedSlot.status === 'unsupported' || selectedSlot.replaceRisk === 'high'}>{translate(locale, `studio.slot.${selectedSlot.status}`, { slot: selectedSlot.runtime })} · {selectedSlot.kind} · {selectedSlot.scope} · {t(`studio.risk.${selectedSlot.replaceRisk}`)}</small></label><label><span>{t('studio.property.parent')}</span><select value={selected.parentId ?? ''} on:change={(event) => updateSelected({ parentId: event.currentTarget.value || undefined })}><option value="">{t('studio.parent.root')}</option>{#each availableParents as item}<option value={item.id}>{item.props.label || item.id}</option>{/each}</select></label><label><span>{t('studio.property.command')}</span><input value={selected.props.command} on:change={(event) => updateSelected({}, { command: event.currentTarget.value })} /></label>{#if selected.type === 'select'}<label><span>{t('studio.property.options')}</span><textarea value={selected.props.options?.join('\n') ?? ''} on:change={(event) => updateSelected({}, { options: event.currentTarget.value.split(/\r?\n/).filter(Boolean) })}></textarea></label>{/if}</div></details>
            <details open><summary>{t('studio.section.layout')}</summary><div class="studio-fieldset"><div class="studio-number-grid"><label><span>X</span><input type="number" min="0" max="10000" step={snap} value={selected.x} on:change={(event) => updateSelected({ x: clampCoordinate(Number(event.currentTarget.value), selected.width) })} /></label><label><span>Y</span><input type="number" min="0" max="10000" step={snap} value={selected.y} on:change={(event) => updateSelected({ y: clampCoordinate(Number(event.currentTarget.value), selected.height) })} /></label><label><span>W</span><input type="number" min="100" max={STUDIO_CANVAS_SIZE - selected.x} step={snap} value={selected.width} on:change={(event) => updateSelected({ width: Math.max(100, Math.min(STUDIO_CANVAS_SIZE - selected.x, Number(event.currentTarget.value))) })} /></label><label><span>H</span><input type="number" min="100" max={STUDIO_CANVAS_SIZE - selected.y} step={snap} value={selected.height} on:change={(event) => updateSelected({ height: Math.max(100, Math.min(STUDIO_CANVAS_SIZE - selected.y, Number(event.currentTarget.value))) })} /></label></div>
              <div class="studio-slider-grid">
                <label class="studio-slider"><span>{t('studio.property.width')}</span><input type="range" min="100" max={Math.max(200, STUDIO_CANVAS_SIZE - selected.x)} step={50} value={selected.width} on:input={(event) => sizeInput({ width: Math.max(100, Math.min(STUDIO_CANVAS_SIZE - selected.x, Number(event.currentTarget.value))) })} on:change={styleCommit} /><output>{selected.width}</output></label>
                <label class="studio-slider"><span>{t('studio.property.height')}</span><input type="range" min="100" max={Math.max(200, STUDIO_CANVAS_SIZE - selected.y)} step={50} value={selected.height} on:input={(event) => sizeInput({ height: Math.max(100, Math.min(STUDIO_CANVAS_SIZE - selected.y, Number(event.currentTarget.value))) })} on:change={styleCommit} /><output>{selected.height}</output></label>
              </div>
              <label class="studio-check"><span>{t('studio.property.disabled')}</span><input type="checkbox" checked={selected.props.disabled} on:change={(event) => updateSelected({}, { disabled: event.currentTarget.checked })} /><i></i></label><label class="studio-check"><span>{t('studio.property.locked')}</span><input type="checkbox" checked={selected.locked} on:change={(event) => updateSelected({ locked: event.currentTarget.checked })} /><i></i></label></div></details>
            <details open><summary>{t('studio.section.style')}</summary><div class="studio-fieldset"><label><span>{t('studio.property.class')}</span><input value={selected.style.className} on:change={(event) => updateStyle({ className: event.currentTarget.value })} /></label>
              <div class="studio-slider-grid">
                <label class="studio-slider"><span>{t('studio.property.fontSize')}</span><input type="range" min="7" max="48" step="1" value={selected.style.fontSize} on:input={(event) => styleInput({ fontSize: Number(event.currentTarget.value) })} on:change={styleCommit} /><output>{selected.style.fontSize}</output></label>
                <label class="studio-slider"><span>{t('studio.property.radius')}</span><input type="range" min="0" max="40" step="1" value={selected.style.radius} on:input={(event) => styleInput({ radius: Number(event.currentTarget.value) })} on:change={styleCommit} /><output>{selected.style.radius}</output></label>
                <label class="studio-slider"><span>{t('studio.property.padding')}</span><input type="range" min="0" max="40" step="1" value={selected.style.padding} on:input={(event) => styleInput({ padding: Number(event.currentTarget.value) })} on:change={styleCommit} /><output>{selected.style.padding}</output></label>
                <label class="studio-slider"><span>{t('studio.property.opacity')}</span><input type="range" min="0" max="100" step="1" value={selected.style.opacity} on:input={(event) => styleInput({ opacity: Number(event.currentTarget.value) })} on:change={styleCommit} /><output>{selected.style.opacity}</output></label>
              </div>
              <div class="studio-number-grid"><label><span>{t('studio.property.opacity')}</span><input type="number" min="0" max="100" value={selected.style.opacity} on:change={(event) => updateStyle({ opacity: Number(event.currentTarget.value) })} /></label><label><span>{t('studio.property.radius')}</span><input type="number" min="0" max="80" value={selected.style.radius} on:change={(event) => updateStyle({ radius: Number(event.currentTarget.value) })} /></label><label><span>{t('studio.property.padding')}</span><input type="number" min="0" max="80" value={selected.style.padding} on:change={(event) => updateStyle({ padding: Number(event.currentTarget.value) })} /></label><label><span>{t('studio.property.fontSize')}</span><input type="number" min="7" max="96" value={selected.style.fontSize} on:change={(event) => updateStyle({ fontSize: Number(event.currentTarget.value) })} /></label></div><label><span>{t('studio.property.weight')}</span><select value={selected.style.fontWeight} on:change={(event) => updateStyle({ fontWeight: Number(event.currentTarget.value) })}>{#each [300, 400, 500, 600, 700, 800] as weight}<option value={weight}>{weight}</option>{/each}</select></label><label><span>{t('studio.property.align')}</span><select value={selected.style.textAlign} on:change={(event) => updateStyle({ textAlign: event.currentTarget.value as StudioComponent['style']['textAlign'] })}><option value="left">Left</option><option value="center">Center</option><option value="right">Right</option></select></label><label><span>{t('studio.property.foreground')}</span><input value={selected.style.foreground} on:change={(event) => updateStyle({ foreground: event.currentTarget.value })} /></label><label><span>{t('studio.property.background')}</span><input value={selected.style.background} on:change={(event) => updateStyle({ background: event.currentTarget.value })} /></label><label><span>{t('studio.property.border')}</span><input value={selected.style.border} on:change={(event) => updateStyle({ border: event.currentTarget.value })} /></label><label><span>{t('studio.property.shadow')}</span><input value={selected.style.shadow} on:change={(event) => updateStyle({ shadow: event.currentTarget.value })} /></label></div></details>
            <details><summary><span>{t('studio.section.binding')}</span><Link2 size={12} /></summary><div class="studio-fieldset"><label><span>{t('studio.property.bindingKey')}</span><input value={selected.binding.key} placeholder="state.selection" on:change={(event) => updateBinding({ key: event.currentTarget.value })} /></label><label><span>{t('studio.property.bindingMode')}</span><select value={selected.binding.mode} on:change={(event) => updateBinding({ mode: event.currentTarget.value as StudioComponent['binding']['mode'] })}><option value="read">{t('studio.binding.read')}</option><option value="write">{t('studio.binding.write')}</option><option value="twoWay">{t('studio.binding.twoWay')}</option></select></label><label><span>{t('studio.property.defaultValue')}</span><input value={selected.binding.defaultValue} on:change={(event) => updateBinding({ defaultValue: event.currentTarget.value })} /></label></div></details>
            <details open><summary><span>{t('studio.section.actions')}</span><small>{selected.actions.length}</small></summary><div class="studio-fieldset studio-action-list"><p class="studio-channel-status"><Zap size={11} />{target === 'dsh' ? t('studio.action.channel.dshLocal') : t('studio.action.channel.knightframe')}</p>{#each selected.actions as action (action.id)}<div class="studio-action-row"><select aria-label={t('studio.action.event')} value={action.event} on:change={(event) => updateAction(action.id, { event: event.currentTarget.value as StudioEventName })}>{#each eventNames as name}<option value={name}>{name}</option>{/each}</select><select aria-label={t('studio.action.type')} value={action.action} on:change={(event) => updateAction(action.id, { action: event.currentTarget.value as StudioActionType })}>{#each actionTypes as name}<option value={name}>{name}</option>{/each}</select><input aria-label={t('studio.action.value')} value={action.value} placeholder={t('studio.action.value')} on:change={(event) => updateAction(action.id, { value: event.currentTarget.value })} /><button type="button" on:click={() => removeAction(action.id)} aria-label={t('studio.action.remove')}><X size={13} /></button></div>{/each}<button class="studio-add-action" type="button" on:click={addAction}><Plus size={13} />{t('studio.action.add')}</button></div></details>
          </div>
        {:else}<div class="studio-inspector-form studio-document-form"><label><span>{t('studio.property.name')}</span><input bind:value={pluginName} placeholder={t('studio.untitled')} /></label><label><span>{t('studio.property.id')}</span><input bind:value={pluginId} /></label><dl><div><dt>{t('studio.property.target')}</dt><dd>{target === 'dsh' ? 'DSH / Cordis' : 'KnightFrame'}</dd></div><div><dt>{t('studio.property.components')}</dt><dd>{components.length}</dd></div><div><dt>{t('studio.property.slots')}</dt><dd>{slots.length}</dd></div></dl></div>{/if}
      </aside>
    </div>
  {:else if view === 'code'}
    <div class="studio-code-workspace"><aside class="studio-code-files"><div class="studio-panel-heading"><span>{t('studio.view.code')}</span><Code2 size={14} /></div>{#each codeFiles as file}<button class:active={activeCode === file.id} type="button" data-code-file={file.id} on:click={() => activeCode = file.id}><Braces size={14} /><span>{t(file.key)}</span><small>{file.editable ? t('studio.code.write') : t('studio.code.read')}</small></button>{/each}</aside><main class="studio-editor"><header><div><strong>{t(codeFiles.find((file) => file.id === activeCode)?.key ?? 'studio.code.layout')}</strong><span>{codeFiles.find((file) => file.id === activeCode)?.editable ? t('studio.code.editable') : t('studio.code.generated')}</span></div>{#if codeFiles.find((file) => file.id === activeCode)?.editable}<button type="button" on:click={applyCodeSource}><Check size={15} />{t('studio.code.apply')}</button>{/if}</header>{#if activeCode === 'layout'}<textarea bind:value={layoutSource} data-source="layout" spellcheck="false" aria-label={t('studio.view.code')}></textarea>{:else if activeCode === 'style'}<textarea bind:value={styleSource} data-source="style" spellcheck="false" aria-label={t('studio.view.code')}></textarea>{:else if activeCode === 'controller'}<textarea bind:value={controllerSource} data-source="controller" spellcheck="false" aria-label={t('studio.view.code')}></textarea>{:else}<textarea class="readonly" value={codeSource} readonly data-source={activeCode} spellcheck="false" aria-label={t('studio.view.code')}></textarea>{/if}</main></div>
  {:else}
    <div class="studio-assistant-workspace"><section class="studio-assistant-context"><span>{t('studio.ask.context')}</span><h2>{document.name}</h2><dl><div><dt>{t('studio.property.target')}</dt><dd>{target === 'dsh' ? t('studio.target.dsh') : 'KnightFrame'}</dd></div><div><dt>{t('studio.property.components')}</dt><dd>{components.length}</dd></div><div><dt>{t('studio.property.slots')}</dt><dd>{new Set(components.map((item) => item.slot)).size}</dd></div><div><dt>{t('studio.ask.selected')}</dt><dd>{selected?.id ?? t('studio.ask.none')}</dd></div></dl></section><form class="studio-assistant-form" on:submit|preventDefault={sendRequest}><label for="studio-requirement">{t('studio.ask')}</label><textarea id="studio-requirement" bind:value={requirement} placeholder={t('studio.ask.placeholder')}></textarea><footer><p>{t('studio.ask.detail')}</p><button type="submit" disabled={!requirement.trim() || sending}><Send size={16} />{t('studio.ask.send')}</button></footer></form></div>
  {/if}
  {#if contextMenu && contextTarget()}
    <div class="studio-context-menu" style={`left:${Math.min(contextMenu.x, window.innerWidth - 240)}px;top:${Math.min(contextMenu.y, window.innerHeight - 300)}px`}>
      {#if contextInput}
        <div class="studio-context-input">
          {#if contextInput.mode === 'page'}
            <select bind:value={contextInput.value} use:initFocus aria-label={t('studio.context.openAppPage')}>
              {#each uiPages as page (page)}<option value={page}>{page}</option>{/each}
            </select>
          {:else}
            <input
              placeholder={contextInput.mode === 'url' ? 'https://…' : t('studio.context.notifyPlaceholder')}
              bind:value={contextInput.value}
              use:initFocus
            />
          {/if}
          <button type="button" on:click={applyContextInput}><Check size={13} /></button>
        </div>
        <small>{t(contextInput.mode === 'url' ? 'studio.context.urlHint' : contextInput.mode === 'page' ? 'studio.context.pageHint' : 'studio.context.notifyHint')}</small>
      {:else}
        <div class="studio-context-title">{contextTarget()?.props.label || contextTarget()?.type}</div>
        <div class="studio-context-caption">{t('studio.context.behavior')}</div>
        <button type="button" on:click={() => beginContextInput('page')}><PanelTop size={13} />{t('studio.context.openAppPage')}</button>
        <button type="button" on:click={() => beginContextInput('url')}><Link2 size={13} />{t('studio.context.openUrl')}</button>
        <button type="button" on:click={() => beginContextInput('notify')}><Zap size={13} />{t('studio.context.notify')}</button>
        <button type="button" on:click={clearContextBehavior}><X size={13} />{t('studio.context.clearBehavior')}</button>
        <div class="studio-context-separator"></div>
        <button type="button" on:click={() => { const item = contextTarget(); closeContextMenu(); if (item) patchComponent(item.id, { locked: !item.locked }); }}>
          {#if contextTarget()?.locked}<Unlock size={13} />{:else}<Lock size={13} />{/if}{t(contextTarget()?.locked ? 'studio.layer.unlock' : 'studio.layer.lock')}
        </button>
        <button type="button" on:click={() => { const item = contextTarget(); closeContextMenu(); if (item) { selectedId = item.id; duplicateSelected(); } }}><Copy size={13} />{t('studio.duplicate')}</button>
        <button type="button" on:click={() => { const item = contextTarget(); closeContextMenu(); if (item) { selectedId = item.id; reorder('front'); } }}><ArrowUpToLine size={13} />{t('studio.front')}</button>
        <button type="button" on:click={() => { const item = contextTarget(); closeContextMenu(); if (item) { selectedId = item.id; reorder('back'); } }}><ArrowDownToLine size={13} />{t('studio.back')}</button>
        <button class="danger" type="button" on:click={() => { const item = contextTarget(); closeContextMenu(); if (item) { selectedId = item.id; removeSelected(); } }}><Trash2 size={13} />{t('studio.delete')}</button>
      {/if}
    </div>
  {/if}
  {#if paletteDrag}
    <div class="studio-palette-ghost" class:over={paletteDrag.over} style={`left:${paletteDrag.ghostX + 14}px;top:${paletteDrag.ghostY + 14}px`}>{paletteDrag.label}</div>
  {/if}
  {#if dialog}
    <div class="studio-dialog-mask" role="presentation" on:pointerdown={() => (dialog = undefined)}>
      <div class="studio-dialog" role="alertdialog" aria-modal="true" aria-label={t('studio.dialog.title')} tabindex={-1} on:pointerdown|stopPropagation>
        <strong>{t('studio.dialog.title')}</strong>
        <p>{dialog.message}</p>
        <button type="button" on:click={() => (dialog = undefined)}>{t('studio.dialog.close')}</button>
      </div>
    </div>
  {/if}
  {#if notice}<div class:error={noticeError} class="studio-notice" role="status">{notice}</div>{/if}
</section>
