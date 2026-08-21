import { get, writable } from 'svelte/store';
import { bridge } from './bridge';
import type {
  AnalysisRecord,
  KlineFrame,
  LocalizedError,
  MarketChatMessage,
  MarketRecordSummary,
  MarketSettings,
  RuntimeEvent,
} from './types';

export interface MarketStageState {
  status: 'idle' | 'running' | 'retry' | 'done' | 'failed';
  reasoning: string;
  content: string;
  retries: { attempt: number; message: string }[];
}

export interface MarketPromptState {
  stage1System: string;
  stage1User: string;
  stage2System: string;
  stage2User: string;
}

export interface MarketState {
  settings?: MarketSettings;
  settingsLoaded: boolean;
  frame?: KlineFrame;
  frameSource?: string;
  live: boolean;
  statusMessage?: string;
  statusError: boolean;
  analyzing: boolean;
  incremental: boolean;
  gateWait: boolean;
  stage1: MarketStageState;
  stage2: MarketStageState;
  prompts: MarketPromptState;
  strategyFiles: string[];
  record?: AnalysisRecord;
  recordId?: string;
  chat: MarketChatMessage[];
  chatDraft: MarketChatMessage | undefined;
  chatStreaming: boolean;
  records: MarketRecordSummary[];
  recordsLoading: boolean;
  error?: LocalizedError;
}

const emptyStage = (): MarketStageState => ({
  status: 'idle',
  reasoning: '',
  content: '',
  retries: [],
});

const defaults: MarketState = {
  settingsLoaded: false,
  live: false,
  statusError: false,
  analyzing: false,
  incremental: false,
  gateWait: false,
  stage1: emptyStage(),
  stage2: emptyStage(),
  prompts: { stage1System: '', stage1User: '', stage2System: '', stage2User: '' },
  strategyFiles: [],
  chat: [],
  chatDraft: undefined,
  chatStreaming: false,
  records: [],
  recordsLoading: false,
};

export const market = writable<MarketState>(defaults);

// 会话内市场工具的图表快照：market.tool_chart 事件按工具调用 id 存档，
// MarketChartCard 直接读全量 K 线渲染（展示不省 token）。
export interface MarketToolChart {
  frame: KlineFrame;
  source: string;
}
export const marketToolCharts = writable<Record<string, MarketToolChart>>({});

function rememberToolChart(event: RuntimeEvent): void {
  const callId = typeof event.data.callId === 'string' ? event.data.callId : undefined;
  const frame = event.data.frame as KlineFrame | undefined;
  if (!callId || !frame) return;
  const source = typeof event.data.source === 'string' ? event.data.source : '';
  marketToolCharts.update((map) => {
    const next = { ...map, [callId]: { frame, source } };
    const ids = Object.keys(next);
    // 会话长流程会积累很多次行情调用：保留最近 16 张图
    while (ids.length > 16) {
      const oldest = ids.shift();
      if (oldest) delete next[oldest];
    }
    return next;
  });
}

function setStage(state: MarketState, stage: 'stage1' | 'stage2', patch: Partial<MarketStageState>): MarketState {
  return { ...state, [stage]: { ...state[stage], ...patch } };
}

function asString(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined;
}

/** 处理 kf://runtime 中的 market.* 事件（由 state.ts 分发）。 */
export function handleMarketEvent(event: RuntimeEvent): void {
  const data = event.data;
  if (event.kind === 'market.tool_chart') {
    rememberToolChart(event);
    return;
  }
  market.update((state) => {
    switch (event.kind) {
      case 'market.frame': {
        const frame = data.frame as KlineFrame | undefined;
        if (!frame) return state;
        return { ...state, frame, frameSource: asString(data.source) ?? state.frameSource };
      }
      case 'market.status': {
        return {
          ...state,
          statusMessage: asString(data.message),
          statusError: data.error === true,
        };
      }
      case 'market.stage': {
        const stage = asString(data.stage) === 'stage2' ? 'stage2' : 'stage1';
        const stateName = asString(data.state);
        if (stateName === 'started') {
          return setStage(state, stage, { status: 'running', reasoning: '', content: '', retries: [] });
        }
        if (stateName === 'retry') {
          const retries = [...state[stage].retries, {
            attempt: typeof data.attempt === 'number' ? data.attempt : 0,
            message: asString(data.message) ?? '',
          }];
          return setStage(state, stage, { status: 'retry', retries });
        }
        return setStage(state, stage, { status: 'done' });
      }
      case 'market.stream': {
        const stage = asString(data.stage) === 'stage2' ? 'stage2' : 'stage1';
        const chunk = asString(data.chunk) ?? '';
        const kind = asString(data.kind);
        if (kind === 'reasoning') {
          return setStage(state, stage, { reasoning: state[stage].reasoning + chunk });
        }
        return setStage(state, stage, { content: state[stage].content + chunk });
      }
      case 'market.prompt': {
        const stage = asString(data.stage) === 'stage2' ? 'stage2' : 'stage1';
        const prompts = { ...state.prompts };
        if (stage === 'stage1') {
          prompts.stage1System = asString(data.system) ?? '';
          prompts.stage1User = asString(data.user) ?? '';
        } else {
          prompts.stage2System = asString(data.system) ?? '';
          prompts.stage2User = asString(data.user) ?? '';
        }
        return { ...state, prompts };
      }
      case 'market.files': {
        const files = Array.isArray(data.files) ? data.files.filter((item): item is string => typeof item === 'string') : [];
        return { ...state, strategyFiles: files };
      }
      case 'market.done': {
        const record = data.record as AnalysisRecord | undefined;
        const gateResult = asString(
          (record?.stage2Decision as Record<string, unknown> | undefined)?.gate_result,
        );
        return {
          ...state,
          analyzing: false,
          gateWait: gateResult === 'wait',
          record,
          recordId: asString(data.recordId) ?? state.recordId,
          incremental: data.incremental === true,
          statusMessage: undefined,
          statusError: false,
        };
      }
      case 'market.failed': {
        return {
          ...state,
          analyzing: false,
          statusMessage: asString(data.message),
          statusError: true,
        };
      }
      case 'market.chat.delta': {
        const kind = asString(data.kind);
        const chunk = asString(data.chunk) ?? '';
        const draft = state.chatDraft ?? { id: crypto.randomUUID(), role: 'assistant', content: '', reasoning: '' };
        if (kind === 'reasoning') {
          return { ...state, chatDraft: { ...draft, reasoning: (draft.reasoning ?? '') + chunk } };
        }
        return { ...state, chatDraft: { ...draft, content: draft.content + chunk } };
      }
      case 'market.chat.done': {
        const draft = state.chatDraft;
        const finalized: MarketChatMessage = draft
          ? { ...draft, content: asString(data.content) ?? draft.content }
          : { id: crypto.randomUUID(), role: 'assistant', content: asString(data.content) ?? '' };
        return {
          ...state,
          chatStreaming: false,
          chatDraft: undefined,
          chat: [...state.chat, finalized],
        };
      }
      case 'market.chat.failed': {
        const draft = state.chatDraft;
        return {
          ...state,
          chatStreaming: false,
          chatDraft: undefined,
          chat: draft ? [...state.chat, draft] : state.chat,
          statusMessage: asString(data.message),
          statusError: true,
        };
      }
      default:
        return state;
    }
  });
}

export async function initMarket(): Promise<void> {
  const state = get(market);
  if (state.settingsLoaded) return;
  try {
    const settings = await bridge.marketSettingsGet();
    market.update((current) => ({ ...current, settings, settingsLoaded: true, error: undefined }));
    void loadMarketRecords();
  } catch (cause) {
    const error = cause as LocalizedError;
    market.update((current) => ({ ...current, settingsLoaded: true, error: error?.key ? error : { key: 'error.market_no_data' } }));
  }
}

export async function updateMarketSettings(settings: MarketSettings): Promise<void> {
  try {
    const saved = await bridge.marketSettingsUpdate(settings);
    market.update((state) => ({ ...state, settings: saved, error: undefined }));
  } catch (cause) {
    const error = cause as LocalizedError;
    market.update((state) => ({
      ...state,
      error: error?.key ? error : { key: 'error.market_settings_write' },
    }));
  }
}

export async function fetchMarketData(input: {
  source: string;
  symbol: string;
  exchange?: string;
  timeframe: string;
}): Promise<void> {
  market.update((state) => ({ ...state, statusMessage: undefined, statusError: false }));
  try {
    await bridge.marketFetch({ ...input, nBars: get(market).settings?.general.analysisBarCount });
  } catch (cause) {
    const error = cause as LocalizedError;
    market.update((state) => ({
      ...state,
      error: error?.key ? error : { key: 'error.market_fetch' },
      statusError: true,
    }));
  }
}

export async function toggleMarketLive(input: {
  source: string;
  symbol: string;
  exchange?: string;
  timeframe: string;
}): Promise<void> {
  const state = get(market);
  if (state.live) {
    await bridge.marketUnsubscribe();
    market.update((current) => ({ ...current, live: false }));
    return;
  }
  try {
    await bridge.marketSubscribe(input);
    market.update((current) => ({ ...current, live: true, error: undefined }));
  } catch (cause) {
    const error = cause as LocalizedError;
    market.update((current) => ({ ...current, error: error?.key ? error : { key: 'error.market_fetch' } }));
  }
}

export async function startMarketAnalysis(forceIncremental = false): Promise<void> {
  market.update((state) => ({
    ...state,
    analyzing: true,
    gateWait: false,
    incremental: forceIncremental,
    stage1: emptyStage(),
    stage2: emptyStage(),
    prompts: { stage1System: '', stage1User: '', stage2System: '', stage2User: '' },
    strategyFiles: [],
    chat: [],
    chatDraft: undefined,
    error: undefined,
    statusMessage: undefined,
    statusError: false,
  }));
  try {
    await bridge.marketAnalyze(forceIncremental);
  } catch (cause) {
    const error = cause as LocalizedError;
    market.update((state) => ({
      ...state,
      analyzing: false,
      error: error?.key ? error : { key: 'error.market_analysis_busy' },
    }));
  }
}

export async function stopMarketAnalysis(): Promise<void> {
  await bridge.marketStopAnalysis();
  market.update((state) => ({ ...state, analyzing: false }));
}

export async function sendMarketChat(text: string): Promise<void> {
  const trimmed = text.trim();
  if (!trimmed) return;
  market.update((state) => ({
    ...state,
    chat: [...state.chat, { id: crypto.randomUUID(), role: 'user', content: trimmed }],
    chatStreaming: true,
    chatDraft: undefined,
    error: undefined,
  }));
  try {
    await bridge.marketChatSend(trimmed);
  } catch (cause) {
    const error = cause as LocalizedError;
    market.update((state) => ({ ...state, chatStreaming: false, error: error?.key ? error : { key: 'error.market_chat_busy' } }));
  }
}

export async function stopMarketChat(): Promise<void> {
  await bridge.marketChatStop();
  market.update((state) => ({ ...state, chatStreaming: false }));
}

export async function loadMarketRecords(limit = 50): Promise<void> {
  market.update((state) => ({ ...state, recordsLoading: true }));
  try {
    const records = await bridge.marketRecords(limit);
    market.update((state) => ({ ...state, records, recordsLoading: false }));
  } catch {
    market.update((state) => ({ ...state, recordsLoading: false }));
  }
}

export async function loadMarketRecord(file: string): Promise<void> {
  try {
    await bridge.marketRecordLoad(file);
    market.update((state) => ({
      ...state,
      error: undefined,
      chat: [],
      chatDraft: undefined,
      chatStreaming: false,
    }));
  } catch (cause) {
    const error = cause as LocalizedError;
    market.update((state) => ({ ...state, error: error?.key ? error : { key: 'error.market_record_decode' } }));
  }
}
