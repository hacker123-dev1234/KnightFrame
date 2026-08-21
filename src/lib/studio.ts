export type StudioTarget = 'knightframe' | 'dsh';
export type StudioComponentType =
  | 'button' | 'toggle' | 'text' | 'input' | 'select' | 'separator' | 'panel'
  | 'container' | 'grid' | 'markdown' | 'image' | 'code';
export type StudioBindingMode = 'read' | 'write' | 'twoWay';
export type StudioEventName = 'click' | 'change' | 'input' | 'focus' | 'submit';
export type StudioActionType = 'command' | 'setValue' | 'toggle' | 'notify' | 'openUrl' | 'openPage';

export interface StudioStyle {
  className: string;
  opacity: number;
  radius: number;
  padding: number;
  fontSize: number;
  fontWeight: number;
  textAlign: 'left' | 'center' | 'right';
  foreground: string;
  background: string;
  border: string;
  shadow: string;
}

export interface StudioBinding {
  key: string;
  mode: StudioBindingMode;
  defaultValue: string;
}

export interface StudioEventAction {
  id: string;
  event: StudioEventName;
  action: StudioActionType;
  value: string;
}

export interface StudioComponent {
  id: string;
  type: StudioComponentType;
  slot: string;
  parentId?: string;
  x: number;
  y: number;
  width: number;
  height: number;
  hidden: boolean;
  locked: boolean;
  style: StudioStyle;
  binding: StudioBinding;
  actions: StudioEventAction[];
  props: {
    label: string;
    command: string;
    disabled: boolean;
    options?: string[];
    source?: string;
    alt?: string;
    language?: string;
    columns?: number;
    gap?: number;
  };
}

export interface StudioSources {
  style: string;
  controller: string;
}

export interface StudioDocument {
  protocolVersion: 'knightframe.plugin.v1';
  id: string;
  name: string;
  version: '0.1.0';
  target: StudioTarget;
  canvas: { width: 10_000; height: 10_000 };
  sources: StudioSources;
  ui: StudioComponent[];
}

export interface StudioAskRequest {
  target: StudioTarget;
  selected?: StudioComponent;
  layout: StudioDocument;
  requirement: string;
  content: string;
}

export interface StudioCommandRequest {
  manifestJson: string;
  target: StudioTarget;
  viewport: { width: number; height: number };
}

export interface StudioExportPreview {
  target: StudioTarget;
  manifestJson: string;
  cordisYaml: string;
  clientContributionJson: string;
  dshClientCode: string;
  dshDefineArgumentsJson: string;
  dshRuntime: Record<string, unknown>;
  adapterPackage: string;
  layout: Record<string, unknown>;
  styleCss?: string;
  controllerSource?: string;
  controllerLanguage?: 'javaScript' | 'typeScript';
  capabilities?: Record<string, unknown>[];
  diagnostics?: Record<string, unknown>[];
  dshSlotCatalog?: Record<string, unknown>[];
}

export interface StudioSlot {
  id: string;
  runtime: string;
  status: 'native' | 'approximated' | 'unsupported';
  kind: 'single' | 'list' | 'keyed' | 'chain';
  scope: 'shell' | 'conversation' | 'sidebar' | 'settings' | 'tool' | 'root';
  replaceRisk: 'none' | 'low' | 'high';
}

type PluginUiContribution = Record<string, unknown> & {
  type: StudioComponentType;
  id: string;
  slot: string;
  bounds: { x: number; y: number; width: number; height: number };
};

export const STUDIO_CANVAS_SIZE = 10_000;

// KnightFrame 原生黑白风：与主界面按钮同一套设计令牌（styles.css :root）——
// #141414 面 / #2a2a2a 边 / 11px 圆角 / .5s ease-soft 过渡 / hover 上浮 + 白光 /
// 按压 scale(.965)。曲线内联，保证插件在 KF 之外的宿主里也能长成 KF 的样子。
export const DEFAULT_STUDIO_STYLE = `.kf-plugin-surface {
  --plugin-gap: 12px;
}

.kf-plugin-component {
  transition: color .5s cubic-bezier(.4, 0, .2, 1), border-color .5s cubic-bezier(.4, 0, .2, 1),
    background-color .5s cubic-bezier(.4, 0, .2, 1), box-shadow .65s cubic-bezier(.4, 0, .2, 1),
    transform .5s cubic-bezier(.22, 1, .36, 1), filter .35s ease;
}

.kf-plugin-component button:hover:not(:disabled) {
  border-color: #303030;
  background: #1a1a1a;
  box-shadow: 0 0 18px rgba(255, 255, 255, 0.055);
  transform: translateY(-1px);
}

.kf-plugin-component button:active:not(:disabled) {
  transform: translateY(0) scale(0.965);
}

.kf-plugin-component button:focus-visible,
.kf-plugin-component input:focus-visible,
.kf-plugin-component select:focus-visible {
  outline: 1px solid rgba(255, 255, 255, 0.72);
  outline-offset: 2px;
}

.kf-plugin-component button:disabled,
.kf-plugin-component input:disabled,
.kf-plugin-component select:disabled {
  cursor: not-allowed;
  opacity: 0.38;
}`;

// 兼容宿主主题：使用适配器定义的强调色与聚焦层次。
// 目标为 DSH 时整套替换，导出的 dsh-client-code.js 也带上这套皮肤。
export const DEFAULT_STUDIO_STYLE_DSH = `.kf-plugin-surface {
  --plugin-gap: 12px;
  --dsh-accent: #4d6bfe;
}

.kf-plugin-component {
  transition: color .3s ease, border-color .3s ease, background-color .3s ease,
    box-shadow .45s ease, transform .28s cubic-bezier(.22, 1, .36, 1), filter .28s ease;
}

.kf-plugin-component button {
  color: #e8ecf5;
  border-color: #2e3442;
  background: #191d26;
}

.kf-plugin-component button:hover:not(:disabled) {
  color: #ffffff;
  border-color: #4d6bfe;
  background: #1f2534;
  box-shadow: 0 0 18px rgba(77, 107, 254, 0.35);
  transform: translateY(-1px);
}

.kf-plugin-component button:active:not(:disabled) {
  transform: translateY(0) scale(0.965);
}

.kf-plugin-component input, .kf-plugin-component select {
  color: #e8ecf5;
  border-color: #2e3442;
  background: #14171e;
}

.kf-plugin-component input:focus-visible, .kf-plugin-component select:focus-visible {
  outline: 1px solid #4d6bfe;
  outline-offset: 2px;
  box-shadow: 0 0 14px rgba(77, 107, 254, 0.3);
}

.kf-plugin-component button:disabled,
.kf-plugin-component input:disabled,
.kf-plugin-component select:disabled {
  cursor: not-allowed;
  opacity: 0.38;
}`;

/// 当前目标的基础主题样式（新建文档与切换目标时兜底）。
export function studioBaseStyle(target: StudioTarget): string {
  return target === 'dsh' ? DEFAULT_STUDIO_STYLE_DSH : DEFAULT_STUDIO_STYLE;
}

export const DEFAULT_STUDIO_CONTROLLER = `export function activate(context) {
  context.on('command', ({ command, value }) => {
    // Route declared component actions here.
    console.info('[KnightFrame Plugin]', command, value);
  });
}`;

const KNIGHTFRAME_SLOTS = [
  'shell.overlay', 'sidebar.header', 'sidebar.footer.action', 'conversation.header',
  'conversation.toolbar', 'composer.before', 'composer.after', 'status.footer', 'tool.view.plugin',
] as const;

const DSH_SLOTS = [
  'conversation', 'conversation.chat.assistant-actions', 'conversation.chat.commandview',
  'conversation.chat.node', 'conversation.chat.turnTail', 'conversation.composer',
  'conversation.composer.bar', 'conversation.composer.dock', 'conversation.details.tool',
  'conversation.hero.agentPreset', 'conversation.hero.workspace',
  'conversation.hero.workspace.directoryFlow', 'conversation.input.dock',
  'conversation.input.left', 'conversation.input.model', 'conversation.input.overlay',
  'conversation.input.plan', 'conversation.input.right', 'conversation.session',
  'conversation.session.header', 'conversation.session.header.actions',
  'conversation.session.header.utilities', 'conversation.view', 'details', 'root',
  'settings.action', 'settings.close', 'settings.general.item', 'settings.header',
  'settings.onboarding', 'settings.plugin.item', 'settings.plugins.tab', 'settings.section',
  'settings.trigger', 'shell.overlay', 'sidebar.footer.action', 'sidebar.settings',
  'sidebar.workspaces', 'sidebar.workspaces.directoryFlow', 'tool.call.toolview',
  'tool.view.cordis',
] as const;

export function studioSlots(target: StudioTarget): StudioSlot[] {
  const catalog: readonly string[] = target === 'dsh' ? DSH_SLOTS : KNIGHTFRAME_SLOTS;
  return catalog.map((id) => {
    const runtime = runtimeStudioSlot(target, id);
    const native = id === runtime || (id === 'tool.view.plugin' && runtime === 'tool.view.cordis');
    const scope = id.startsWith('conversation') || id.startsWith('composer')
      ? 'conversation'
      : id.startsWith('sidebar') ? 'sidebar'
      : id.startsWith('settings') ? 'settings'
      : id.startsWith('tool') ? 'tool'
      : id === 'root' ? 'root' : 'shell';
    const replaceRisk = id === 'root' || id === 'conversation' || id === 'conversation.view' || id === 'details'
      ? 'high'
      : id.endsWith('.header') || id.endsWith('.overlay') ? 'low' : 'none';
    const kind = id.endsWith('.item') || id.endsWith('.action') || id.endsWith('.actions')
      ? 'list'
      : id.includes('directoryFlow') || id.endsWith('.utilities') ? 'chain'
      : id.endsWith('.tab') || id.endsWith('.toolview') ? 'keyed' : 'single';
    return { id, runtime, status: native ? 'native' : 'approximated', kind, scope, replaceRisk };
  });
}

export function studioSlotCapability(target: StudioTarget, slot: string): StudioSlot {
  return studioSlots(target).find((item) => item.id === slot) ?? {
    id: slot,
    runtime: runtimeStudioSlot(target, slot),
    status: 'unsupported',
    kind: 'single',
    scope: 'shell',
    replaceRisk: 'high',
  };
}

export function runtimeStudioSlot(target: StudioTarget, slot: string): string {
  if (slot === 'shell.overlay') return 'shell.overlay';
  if (slot === 'sidebar.footer.action') return 'sidebar.footer.action';
  if (slot === 'tool.view.plugin' || slot === 'tool.view.cordis' || slot === 'conversation.toolbar') {
    return target === 'dsh' ? 'tool.view.cordis' : 'tool.view.plugin';
  }
  if (slot.startsWith('sidebar.')) return 'sidebar.footer.action';
  return 'shell.overlay';
}

export function clampCoordinate(value: number, extent: number): number {
  const finite = Number.isFinite(value) ? Math.round(value) : 0;
  return Math.max(0, Math.min(STUDIO_CANVAS_SIZE - Math.max(1, extent), finite));
}

// 参考舞台 1280×720（10K 画布 ⇒ 1 x 单位 = 0.128px，1 y 单位 = 0.072px）。
// 默认尺寸按 KF 原生控件真实度量换算：按钮/输入框 ~46px 高、42px 侧栏按钮、
// 文本 15px 行——工坊里放出来就是 KF 界面里的原生比例。
export const STUDIO_STAGE_WIDTH = 1280;
export const STUDIO_STAGE_HEIGHT = 720;
export const STUDIO_BASE_FONT = 12;

export function defaultStudioStyle(type: StudioComponentType, target: StudioTarget = 'knightframe'): StudioStyle {
  // KF 原版令牌：#141414 面（--surface-2）/ #2a2a2a 边（--border）/ 11px 圆角（--radius-sm）。
  // DSH：品牌蓝灰面板色。
  const dsh = target === 'dsh';
  return {
    className: '',
    opacity: 100,
    radius: type === 'separator' || type === 'text' ? 0 : dsh ? 10 : 11,
    padding: type === 'panel' ? 12 : 8,
    fontSize: type === 'text' ? 15 : type === 'panel' ? 13 : 13,
    fontWeight: type === 'text' ? 500 : type === 'button' ? 500 : 400,
    textAlign: type === 'input' ? 'left' : 'center',
    foreground: dsh ? '#e8ecf5' : '#d8d8d8',
    background: type === 'separator'
      ? (dsh ? '#3a4152' : '#666666')
      : type === 'text'
        ? 'transparent'
        : dsh ? '#191d26' : '#141414',
    border: type === 'text' || type === 'separator'
      ? 'transparent'
      : dsh ? '#2e3442' : '#2a2a2a',
    shadow: 'none',
  };
}

export function createStudioAction(): StudioEventAction {
  return { id: crypto.randomUUID(), event: 'click', action: 'command', value: '' };
}

export function createStudioComponent(type: StudioComponentType, index: number, label: string, target: StudioTarget = 'knightframe'): StudioComponent {
  const structural = type === 'panel' || type === 'container' || type === 'grid';
  // 参考舞台 1280×720 下的原生度量：按钮/输入 46px 高（640 y 单位）、
  // 文本 ~37px 行、面板 538×202px；分隔线 2px 视觉高度。
  const width = type === 'separator' ? 2800
    : structural ? 4200
    : type === 'text' || type === 'markdown' ? 3000
    : type === 'image' ? 2800
    : type === 'code' ? 3600
    : 2400;
  const height = type === 'separator' ? 280
    : structural ? 2800
    : type === 'text' ? 520
    : type === 'markdown' ? 1800
    : type === 'code' ? 1600
    : type === 'image' ? 2000
    : 640;
  const offset = (index % 6) * 320;
  return {
    id: `${type}-${crypto.randomUUID()}`,
    type,
    slot: 'shell.overlay',
    x: clampCoordinate(3600 + offset, width),
    y: clampCoordinate(3600 + offset, height),
    width,
    height,
    hidden: false,
    locked: false,
    style: defaultStudioStyle(type, target),
    binding: { key: '', mode: 'twoWay', defaultValue: '' },
    actions: [],
    props: {
      label,
      command: '',
      disabled: false,
      options: type === 'select' ? ['Option A', 'Option B'] : undefined,
      source: type === 'image' ? 'assets/plugin-image.png' : undefined,
      alt: type === 'image' ? label : undefined,
      language: type === 'code' ? 'text' : undefined,
      columns: type === 'grid' ? 2 : undefined,
      gap: type === 'grid' ? 12 : undefined,
    },
  };
}

export function normalizeStudioComponent(component: StudioComponent): StudioComponent {
  const fallback = defaultStudioStyle(component.type);
  const binding = component.binding ?? { key: '', mode: 'twoWay' as const, defaultValue: '' };
  const props = component.props ?? { label: component.type, command: '', disabled: false };
  return {
    ...component,
    slot: component.slot || 'shell.overlay',
    hidden: Boolean(component.hidden),
    locked: Boolean(component.locked),
    style: { ...fallback, ...(component.style ?? {}) },
    binding: {
      key: binding.key ?? '',
      mode: binding.mode ?? 'twoWay',
      defaultValue: binding.defaultValue ?? '',
    },
    actions: Array.isArray(component.actions) ? component.actions : [],
    props: {
      ...props,
      label: props.label ?? component.type,
      command: props.command ?? '',
      disabled: Boolean(props.disabled),
    },
  };
}

export function studioDocument(
  id: string,
  name: string,
  target: StudioTarget,
  ui: StudioComponent[],
  sources: StudioSources = { style: DEFAULT_STUDIO_STYLE, controller: DEFAULT_STUDIO_CONTROLLER },
): StudioDocument {
  return {
    protocolVersion: 'knightframe.plugin.v1',
    id: id.trim() || 'local.untitled',
    name: name.trim() || 'Untitled',
    version: '0.1.0',
    target,
    canvas: { width: STUDIO_CANVAS_SIZE, height: STUDIO_CANVAS_SIZE },
    sources,
    ui,
  };
}

function command(value: string): { command?: string } {
  const trimmed = value.trim();
  return trimmed ? { command: trimmed } : {};
}

function actionMap(component: StudioComponent): Record<string, Record<string, unknown>[]> {
  const result: Record<string, Record<string, unknown>[]> = {};
  for (const item of component.actions) {
    const path = component.binding.key || 'state.value';
    let action: Record<string, unknown>;
    if (item.action === 'command') {
      action = { type: 'command', command: item.value || component.props.command || 'plugin.action', arguments: {} };
    } else if (item.action === 'setValue') {
      action = { type: 'setData', path, value: item.value };
    } else if (item.action === 'toggle') {
      action = { type: 'toggleData', path };
    } else if (item.action === 'openUrl') {
      action = { type: 'emit', event: 'openUrl', payload: { url: item.value } };
    } else if (item.action === 'openPage') {
      // 驱动 KnightFrame 原生 UI 层：切换到指定页面（workspace/market/…）
      action = { type: 'emit', event: 'openPage', payload: { page: item.value } };
    } else {
      action = { type: 'emit', event: item.value || 'notify', payload: {} };
    }
    (result[item.event] ??= []).push(action);
  }
  return result;
}

function contribution(component: StudioComponent, target: StudioTarget, all: StudioComponent[]): PluginUiContribution {
  const children = all.filter((item) => item.parentId === component.id).map((item) => item.id);
  // 尺寸类样式以 em 下发（相对宿主舞台基准字号 12px@1280）：
  // 舞台放大缩小（KF 覆盖层 / DSH 画布 / 工坊画布）时控件文字与留白同步缩放，
  // 永远和工坊里看到的比例一致。
  const em = (value: number) => `${Math.round((value / STUDIO_BASE_FONT) * 1000) / 1000}em`;
  const style = Object.fromEntries(Object.entries({
    opacity: `${Math.max(0, Math.min(100, component.style.opacity)) / 100}`,
    borderRadius: em(Math.max(0, component.style.radius)),
    padding: em(Math.max(0, component.style.padding)),
    fontSize: em(Math.max(7, component.style.fontSize)),
    fontWeight: `${component.style.fontWeight}`,
    textAlign: component.style.textAlign,
    color: component.style.foreground,
    background: component.style.background,
    borderColor: component.style.border,
    boxShadow: component.style.shadow,
  }).filter(([, value]) => value !== '' && value !== 'none'));
  const base = {
    type: component.type,
    id: component.id,
    slot: runtimeStudioSlot(target, component.slot),
    bounds: {
      x: component.x,
      y: component.y,
      width: component.width,
      height: component.height,
    },
    props: {
      className: component.style.className,
      bindingMode: component.binding.mode,
      hidden: component.hidden,
      locked: component.locked,
    },
    style,
    bindings: component.binding.key
      ? { value: { path: component.binding.key, fallback: component.binding.defaultValue } }
      : {},
    actions: actionMap(component),
  };
  const declaredCommand = component.props.command || component.actions.find((item) => item.action === 'command')?.value || '';
  const label = component.props.label.trim() || component.type;
  switch (component.type) {
    case 'button':
      return { ...base, label, ...command(declaredCommand), disabled: component.props.disabled };
    case 'toggle':
      return { ...base, label, value: component.binding.defaultValue === 'true', ...command(declaredCommand), disabled: component.props.disabled };
    case 'text':
      return { ...base, text: label };
    case 'input':
      return { ...base, label, placeholder: label, value: component.binding.defaultValue, ...command(declaredCommand), disabled: component.props.disabled };
    case 'select': {
      const labels = component.props.options?.filter((value) => value.trim()) ?? [label];
      const options = labels.map((option, index) => ({ value: `option-${index + 1}`, label: option.trim() }));
      return { ...base, label, options, value: component.binding.defaultValue || options[0]?.value, ...command(declaredCommand), disabled: component.props.disabled };
    }
    case 'separator':
      return { ...base, orientation: component.height > component.width ? 'vertical' : 'horizontal' };
    case 'panel':
      return { ...base, title: label, elevated: component.style.shadow !== 'none' };
    case 'container':
      return { ...base, title: label, children };
    case 'grid':
      return { ...base, columns: component.props.columns ?? 2, gap: component.props.gap ?? 12, children };
    case 'markdown':
      return { ...base, content: label };
    case 'image':
      return { ...base, source: component.props.source || 'assets/plugin-image.png', alt: component.props.alt || label };
    case 'code':
      return { ...base, code: label, language: component.props.language || 'text' };
  }
}

export function studioManifest(document: StudioDocument): Record<string, unknown> {
  return {
    protocolVersion: document.protocolVersion,
    id: document.id,
    name: document.name,
    version: document.version,
    runtime: 'command',
    entry: 'bin/plugin-host.exe',
    configSchema: { type: 'object' },
    inject: [],
    provide: [],
    intercept: {},
    isolate: {},
    tools: [],
    ui: document.ui.filter((component) => !component.hidden).map((component) => contribution(component, document.target, document.ui)),
    styleCss: document.sources.style,
    controller: document.sources.controller.trim()
      ? { language: 'javaScript', source: document.sources.controller }
      : undefined,
    permissions: document.sources.controller.trim() ? ['ui', 'uiController'] : ['ui'],
  };
}

export function studioCommandRequest(document: StudioDocument): StudioCommandRequest {
  return {
    manifestJson: JSON.stringify(studioManifest(document)),
    target: document.target,
    viewport: { width: 1280, height: 720 },
  };
}
