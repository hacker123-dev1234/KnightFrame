export type Locale = 'en-US' | 'zh-CN';

export type RuntimeStatus = 'connecting' | 'ready' | 'offline';
export type SessionStatus = 'idle' | 'streaming' | 'failed';
export type ReceiptStatus = 'pending' | 'running' | 'completed' | 'failed' | 'blocked' | 'cancelled' | 'skipped';

export interface LocalizedError {
  key: string;
  args?: Record<string, string>;
}

export interface ProviderModel {
  providerId: string;
  providerName: string;
  modelId: string;
  modelName: string;
  available: boolean;
  capabilities?: string[];
  contextLimit?: number;
  thinkingEnabled: boolean;
  thinkingEffort: ThinkingEffort;
  thinkingToggle: boolean;
  thinkingEfforts: ThinkingEffort[];
  adapter: ProviderAdapter;
}

export type ProviderAdapter = 'openai' | 'openai-responses' | 'anthropic' | 'gemini';
export type ThinkingEffort = 'none' | 'minimal' | 'low' | 'medium' | 'high' | 'xhigh' | 'max';
export interface ConfiguredModel { id: string; name: string; adapter?: ProviderAdapter; capabilities: string[]; contextLimit?: number; thinkingEnabled?: boolean; thinkingEffort?: ThinkingEffort; thinkingToggle?: boolean; thinkingEfforts?: ThinkingEffort[]; catalogSynced?: boolean; }
export interface ProviderProfile { id: string; name: string; adapter: ProviderAdapter; baseUrl: string; userAgent: string; apiKey: string; credentialRef: string; models: ConfiguredModel[]; }
export interface ProviderTemplate { id: string; name: string; adapter: ProviderAdapter; baseUrl: string; apiKeyEnv: string; }

export interface FeatureAvailability {
  browser?: boolean;
  miniAssistant?: boolean;
  usageLedger?: boolean;
  projectIndex?: boolean;
  projectManifest?: boolean;
}

export interface SettingsSnapshot {
  locale: Locale;
  taskManager: boolean;
  cavemanMode: 'lite' | 'off';
  usagePanel?: boolean;
  userAvatar?: string;
  providerId?: string;
  modelId?: string;
  providers: ProviderProfile[];
  auxiliaryEnabled: boolean;
  auxiliaryProviderId?: string;
  auxiliaryModelId?: string;
  uiScale?: number;
}

export interface UsageSnapshot {
  turnId?: string;
  turnFreshInputTokens?: number;
  turnCacheReadTokens?: number;
  turnElapsedMs?: number;
  sessionElapsedMs?: number;
  freshInputTokens?: number;
  cacheReadTokens?: number;
  cacheWriteTokens?: number;
  outputTokens?: number;
  reasoningTokens?: number;
  requestCount?: number;
  currentContextTokens?: number;
  outputTokensPerSecond?: number;
  cost?: { amount: number; currency: string; estimated: boolean };
}

export interface ReasoningBlock {
  id: string;
  summary: string;
  status: ReceiptStatus;
}

export interface ToolReceipt {
  id: string;
  name: string;
  status: ReceiptStatus;
  arguments?: Record<string, unknown>;
  elapsedMs?: number;
  summary?: string;
  diagnostic?: string;
  artifactId?: string;
  projection?: ToolProjectionSnapshot;
}

export interface ToolProjectionSnapshot {
  status?: string;
  summary?: string;
  exitCode?: number;
  errorKey?: string;
  completeness?: string;
  total?: number;
  truncated?: boolean;
  artifactId?: string;
}

export interface AuxiliaryReceipt {
  id: string;
  role: string;
  model?: string;
  status: ReceiptStatus;
  reason?: string;
  beforeTokens?: number;
  afterTokens?: number;
  inputTokens?: number;
  outputTokens?: number;
  elapsedMs?: number;
  summary?: string;
}

export interface MessageSnapshot {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  createdAt?: string;
  reasoning?: ReasoningBlock[];
  tools?: ToolReceipt[];
  auxiliary?: AuxiliaryReceipt[];
  attachments?: MessageAttachment[];
}

export interface MessageAttachment { id: string; name: string; mimeType: string; dataUrl: string; size: number; }

export interface TaskItem {
  id: string;
  title: string;
  detail?: string;
  status: ReceiptStatus;
}

export interface TaskSnapshot {
  id: string;
  status: ReceiptStatus;
  completed: number;
  total: number;
  current?: string;
  items: TaskItem[];
}

export interface ProjectSnapshot {
  root?: string;
  name?: string;
  status: 'unavailable' | 'indexing' | 'ready' | 'updating' | 'stale' | 'failed';
  stage?: string;
  completed?: number;
  total?: number;
  files?: number;
  languages?: string[];
  failures?: number;
  etaMs?: number;
}

export interface GraphNode {
  id: string;
  label: string;
  kind: 'file' | 'directory' | string;
  path: string;
  line?: number;
  weight: number;
}

export interface GraphEdge {
  source: string;
  target: string;
  kind: 'contains' | 'depends' | string;
  weight: number;
}

export interface GraphSnapshot {
  root: string;
  nodes: GraphNode[];
  edges: GraphEdge[];
  stats: { files: number; directories: number; dependencies: number };
}

export interface SessionSnapshot {
  id: string;
  title?: string;
  providerId?: string;
  modelId?: string;
  projectRoot?: string;
  status: SessionStatus;
  lastError?: LocalizedError;
  messages: MessageSnapshot[];
  task?: TaskSnapshot;
  usage?: UsageSnapshot;
}

export interface BrowserPermissionReceipt {
  id: string;
  permission: string;
  status: 'asked' | 'allowed' | 'denied';
}

export interface BrowserTabSnapshot {
  id: string;
  url?: string;
  title?: string;
  canGoBack: boolean;
  canGoForward: boolean;
  loading: boolean;
}

export interface BrowserSnapshot {
  available: boolean;
  open: boolean;
  url?: string;
  title?: string;
  canGoBack?: boolean;
  canGoForward?: boolean;
  loading?: boolean;
  activeTabId?: string;
  tabs: BrowserTabSnapshot[];
  error?: LocalizedError;
  permissions?: BrowserPermissionReceipt[];
}

export interface BootstrapPayload {
  settings: SettingsSnapshot;
  providers: ProviderModel[];
  providerTemplates: ProviderTemplate[];
  sessions: SessionSnapshot[];
  activeSessionId?: string;
  project?: ProjectSnapshot;
  browser?: BrowserSnapshot;
  features?: FeatureAvailability;
}

export interface RuntimeEvent {
  eventId: string;
  sessionId?: string;
  taskId?: string;
  kind:
    | 'provider.model_probe'
    | 'session.started'
    | 'session.renamed'
    | 'session.deleted'
    | 'assistant.reasoning_delta'
    | 'assistant.text_delta'
    | 'assistant.tool_call'
    | 'assistant.usage'
    | 'assistant.completed'
    | 'assistant.cancelled'
    | 'assistant.failed'
    | 'auxiliary.started'
    | 'auxiliary.completed'
    | 'auxiliary.skipped'
    | 'auxiliary.failed'
    | 'skill.activated'
    | 'task.updated'
    | 'project.index_progress'
    | 'project.ready'
    | 'project.failed'
    | 'tool.started'
    | 'tool.completed'
    | 'tool.failed'
    | 'market.frame'
    | 'market.status'
    | 'market.stage'
    | 'market.stream'
    | 'market.prompt'
    | 'market.files'
    | 'market.done'
    | 'market.failed'
    | 'market.chat.delta'
    | 'market.chat.done'
    | 'market.chat.failed'
    | 'market.tool_chart'
    | 'browser.opened'
    | 'browser.updated'
    | 'ui.page';
  data: Record<string, unknown>;
}

export type BrowserAction = 'open' | 'new-tab' | 'select-tab' | 'close-tab' | 'back' | 'forward' | 'refresh' | 'stop' | 'close' | 'navigate' | 'focus' | 'show' | 'hide';

// ---------------------------------------------------------------------------
// Market（PA Agent 移植层）
// ---------------------------------------------------------------------------

export interface KlineBar {
  seq: number;
  tsOpen: number;
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
  closed: boolean;
}

export interface IndicatorBundle {
  ema20: (number | null)[];
  atr14: (number | null)[];
}

export interface KlineFrame {
  symbol: string;
  timeframe: string;
  bars: KlineBar[];
  indicators: IndicatorBundle;
  snapshotTsLocalMs: number;
}

export interface MarketProviderSettings {
  model: string;
  baseUrl: string;
  apiKey: string;
  thinking: boolean;
  reasoningEffort: string;
  contextWindow: number;
}

export interface MarketGeneralSettings {
  analysisBarCount: number;
  refreshIntervalMs: number;
  contextWarningThresholdPct: number;
  lastDataSource: string;
  lastTradingviewExchange: string;
  lastSymbol: string;
  lastTimeframe: string;
  decisionFlowAutoPlay: boolean;
  decisionFlowPlaySeconds: number;
  alertOnOrderOpportunity: boolean;
  incrementalMaxNewBars: number;
  decisionStance: string;
  keepAnalysis: boolean;
  cancelKeepAnalysisOnRetry: boolean;
}

export interface MarketPromptSettings {
  stage2LoadFullStrategyLibrary: boolean;
  experienceMaxEntries: number;
  experienceMaxCharsPerEntry: number;
  stage1InjectPatternBriefs: boolean;
}

export interface MarketValidationSettings {
  normalizationMode: string;
  stage1CoherenceChecks: boolean;
  stage2CoherenceChecks: boolean;
  traceSemanticChecks: boolean;
  disableTruncationRepair: boolean;
  retryEnabled: boolean;
  retryMax: number;
  retryMaxSemantic: number;
  retryStage2: boolean;
}

export interface MarketSettings {
  provider: MarketProviderSettings;
  general: MarketGeneralSettings;
  prompt: MarketPromptSettings;
  validation: MarketValidationSettings;
}

export interface AnalysisRecord {
  meta: Record<string, unknown>;
  /** 后端在 market.done / record_load 负载中剥离 K 线数组（图表走 frame），仅磁盘记录含该字段。 */
  klineData?: KlineBar[];
  htfText?: string;
  stage1Messages?: Record<string, unknown>[];
  stage1Response?: Record<string, unknown>;
  stage1Diagnosis?: Record<string, unknown>;
  stage2Messages?: Record<string, unknown>[];
  stage2Response?: Record<string, unknown>;
  stage2Decision?: Record<string, unknown>;
  strategyFilesUsed?: string[];
  experienceLoaded?: Record<string, unknown>[];
  exception?: Record<string, unknown>;
  usageTotal?: Record<string, unknown>;
  partialReason?: string;
}

export interface MarketRecordSummary {
  file: string;
  meta: Record<string, unknown>;
  hasDecision: boolean;
  partial: boolean;
}

export interface MarketSubscription {
  source: string;
  symbol: string;
  exchange: string;
  timeframe: string;
  nBars: number;
  intervalMs: number;
}

export interface MarketFetchInput {
  source: string;
  symbol: string;
  exchange?: string;
  timeframe: string;
  nBars?: number;
}

export interface MarketSubscribeInput {
  source: string;
  symbol: string;
  exchange?: string;
  timeframe?: string;
}

export type MarketStreamKind = 'reasoning' | 'content';

export interface MarketChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  reasoning?: string;
}

export interface KnightFrameBridge {
  bootstrap(): Promise<BootstrapPayload>;
  updateSettings(patch: Partial<SettingsSnapshot>): Promise<SettingsSnapshot>;
  probeProvider(profile: ProviderProfile): Promise<ConfiguredModel[]>;
  createSession(input: { projectRoot?: string; provider?: string; model?: string }): Promise<SessionSnapshot>;
  updateSessionModel(sessionId: string, provider: string, model: string): Promise<SessionSnapshot>;
  renameSession(sessionId: string, title: string): Promise<SessionSnapshot>;
  deleteSession(sessionId: string): Promise<{ ok: boolean }>;
  send(sessionId: string, content: string, clarify?: boolean, attachments?: MessageAttachment[]): Promise<{ turnId: string }>;
  stop(sessionId: string): Promise<{ ok: boolean }>;
  openProject(): Promise<ProjectSnapshot | undefined>;
  projectGraph(root: string): Promise<GraphSnapshot>;
  taskCommand(sessionId: string, op: string, item?: string): Promise<TaskSnapshot>;
  browserCommand(action: BrowserAction, url?: string, tabId?: string): Promise<BrowserSnapshot>;
  browserRect(x: number, y: number, width: number, height: number): Promise<BrowserSnapshot>;
  openPluginStudio(): Promise<{ ok: boolean }>;
  pluginStudioBootstrap(): Promise<{ locale: Locale }>;
  pluginStudioReady(): Promise<{ ok: boolean }>;
  startDshPreview(): Promise<{ available: boolean; running: boolean; url?: string; reason?: string }>;
  stopDshPreview(): Promise<{ ok: boolean }>;
  relayUiPage(page: string): Promise<{ ok: boolean }>;
  pluginStudioPreview(document: import('./studio').StudioDocument): Promise<import('./studio').StudioExportPreview>;
  exportPluginStudio(document: import('./studio').StudioDocument): Promise<{ ok: boolean; path?: string }>;
  askFromPluginStudio(request: import('./studio').StudioAskRequest): Promise<{ ok: boolean }>;
  subscribePluginStudio(listener: (request: { content: string }) => void | Promise<void>): Promise<() => void>;
  subscribe(listener: (event: RuntimeEvent) => void): Promise<() => void>;
  marketSettingsGet(): Promise<MarketSettings>;
  marketSettingsUpdate(settings: MarketSettings): Promise<MarketSettings>;
  marketFetch(input: MarketFetchInput): Promise<{ ok: boolean }>;
  marketSubscribe(input: MarketSubscribeInput): Promise<{ ok: boolean; subscription: MarketSubscription }>;
  marketUnsubscribe(): Promise<{ ok: boolean }>;
  marketAnalyze(forceIncremental?: boolean): Promise<{ ok: boolean }>;
  marketStopAnalysis(): Promise<{ ok: boolean }>;
  marketChatSend(text: string): Promise<{ ok: boolean }>;
  marketChatStop(): Promise<{ ok: boolean }>;
  marketRecords(limit?: number): Promise<MarketRecordSummary[]>;
  marketRecordLoad(file: string): Promise<{ record: AnalysisRecord; frame: KlineFrame }>;
}
