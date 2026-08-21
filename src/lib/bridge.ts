import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { studioCommandRequest, type StudioAskRequest, type StudioDocument, type StudioExportPreview } from './studio';
import type {
  AnalysisRecord,
  BootstrapPayload,
  BrowserAction,
  BrowserSnapshot,
  GraphSnapshot,
  KlineFrame,
  KnightFrameBridge,
  MarketFetchInput,
  MarketRecordSummary,
  MarketSettings,
  MarketSubscribeInput,
  MarketSubscription,
  ProjectSnapshot,
  RuntimeEvent,
  SessionSnapshot,
  SettingsSnapshot,
  TaskSnapshot,
} from './types';

function command<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(name, args);
}

export const bridge: KnightFrameBridge = {
  bootstrap: () => command<BootstrapPayload>('kf_app_bootstrap'),
  updateSettings: (patch: Partial<SettingsSnapshot>) =>
    command<SettingsSnapshot>('kf_settings_update', { patch }),
  probeProvider: (profile) => command('kf_provider_probe', { profile }),
  createSession: (input) => command<SessionSnapshot>('kf_session_create', input),
  updateSessionModel: (sessionId, provider, model) =>
    command<SessionSnapshot>('kf_session_model_update', { sessionId, provider, model }),
  renameSession: (sessionId, title) => command<SessionSnapshot>('kf_session_rename', { sessionId, title }),
  deleteSession: (sessionId) => command<{ ok: boolean }>('kf_session_delete', { sessionId }),
  send: (sessionId, content, clarify = false, attachments = []) =>
    command<{ turnId: string }>('kf_session_send', { sessionId, content, clarify, attachments }),
  stop: (sessionId) => command<{ ok: boolean }>('kf_session_stop', { sessionId }),
  openProject: async () => {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected !== 'string') return undefined;
    return command<ProjectSnapshot>('kf_project_open', { path: selected });
  },
  projectGraph: (root) => command<GraphSnapshot>('kf_project_graph', { root }),
  taskCommand: (sessionId, op, item) =>
    command<TaskSnapshot>('kf_task_command', { sessionId, op, item }),
  browserCommand: (action: BrowserAction, url?: string, tabId?: string) =>
    command<BrowserSnapshot>('kf_browser_command', { action, url, tabId }),
  browserRect: (x: number, y: number, width: number, height: number) =>
    command<BrowserSnapshot>('kf_browser_rect', { x, y, width, height }),
  openPluginStudio: () => command<{ ok: boolean }>('kf_plugin_studio_open'),
  pluginStudioBootstrap: () => command<{ locale: 'en-US' | 'zh-CN' }>('kf_plugin_studio_bootstrap'),
  pluginStudioReady: () => command<{ ok: boolean }>('kf_plugin_studio_ready'),
  startDshPreview: () => command<{ available: boolean; running: boolean; url?: string; reason?: string }>('kf_plugin_studio_dsh_start'),
  stopDshPreview: () => command<{ ok: boolean }>('kf_plugin_studio_dsh_stop'),
  relayUiPage: (page: string) => command<{ ok: boolean }>('kf_plugin_studio_ui_relay', { page }),
  pluginStudioPreview: (document: StudioDocument) =>
    command<StudioExportPreview>('kf_plugin_studio_export_preview', { request: studioCommandRequest(document) }),
  exportPluginStudio: async (document: StudioDocument) => {
    const outputDir = await open({ directory: true, multiple: false });
    if (typeof outputDir !== 'string') return { ok: false };
    return command<{ ok: boolean; path?: string }>('kf_plugin_studio_export', {
      request: studioCommandRequest(document),
      outputDir,
    });
  },
  askFromPluginStudio: (request: StudioAskRequest) => command<{ ok: boolean }>('kf_plugin_studio_ask', {
    request: {
      studio: studioCommandRequest(request.layout),
      content: request.content,
      requirement: request.requirement,
      selectedId: request.selected?.id,
    },
  }),
  subscribePluginStudio: async (listener) => {
    const unlisten = await listen<{ content: string }>('kf://plugin-studio-request', (event) => listener(event.payload));
    return unlisten;
  },
  subscribe: async (listener) => {
    const unlisten = await listen<RuntimeEvent>('kf://runtime', (event) => listener(event.payload));
    return unlisten;
  },
  marketSettingsGet: () => command<MarketSettings>('kf_market_settings_get'),
  marketSettingsUpdate: (settings) => command<MarketSettings>('kf_market_settings_update', { settings }),
  marketFetch: (input) =>
    command<{ ok: boolean }>('kf_market_fetch', {
      source: input.source,
      symbol: input.symbol,
      exchange: input.exchange,
      timeframe: input.timeframe,
      nBars: input.nBars,
    }),
  marketSubscribe: (input) =>
    command<{ ok: boolean; subscription: MarketSubscription }>('kf_market_subscribe', {
      source: input.source,
      symbol: input.symbol,
      exchange: input.exchange,
      timeframe: input.timeframe,
    }),
  marketUnsubscribe: () => command<{ ok: boolean }>('kf_market_unsubscribe'),
  marketAnalyze: (forceIncremental = false) =>
    command<{ ok: boolean }>('kf_market_analyze', { forceIncremental }),
  marketStopAnalysis: () => command<{ ok: boolean }>('kf_market_stop_analysis'),
  marketChatSend: (text) => command<{ ok: boolean }>('kf_market_chat_send', { text }),
  marketChatStop: () => command<{ ok: boolean }>('kf_market_chat_stop'),
  marketRecords: (limit) => command<MarketRecordSummary[]>('kf_market_records', { limit }),
  marketRecordLoad: (file) =>
    command<{ record: AnalysisRecord; frame: KlineFrame }>('kf_market_record_load', { file }),
};
