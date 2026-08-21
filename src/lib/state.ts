import { derived, get, writable } from 'svelte/store';
import { bridge } from './bridge';
import { handleMarketEvent } from './market-state';
import type {
  BootstrapPayload,
  BrowserSnapshot,
  GraphSnapshot,
  LocalizedError,
  MessageSnapshot,
  ProjectSnapshot,
  ProviderModel,
  ProviderTemplate,
  MessageAttachment,
  RuntimeEvent,
  RuntimeStatus,
  SessionSnapshot,
  SettingsSnapshot,
  TaskSnapshot,
  ToolReceipt,
  ToolProjectionSnapshot,
  UsageSnapshot,
} from './types';

export type Page = 'workspace' | 'settings' | 'browser' | 'graph' | 'mini' | 'market';

export interface AppState {
  runtime: RuntimeStatus;
  error?: LocalizedError;
  page: Page;
  sidebarCollapsed: boolean;
  taskPanelOpen: boolean;
  usagePanelOpen: boolean;
  settings: SettingsSnapshot;
  providers: ProviderModel[];
  providerTemplates: ProviderTemplate[];
  sessions: SessionSnapshot[];
  activeSessionId?: string;
  project?: ProjectSnapshot;
  browser?: BrowserSnapshot;
  graph?: GraphSnapshot;
  graphLoading: boolean;
  graphError?: LocalizedError;
  features: BootstrapPayload['features'];
}

const defaults: AppState = {
  runtime: 'connecting',
  page: 'workspace',
  sidebarCollapsed: false,
  taskPanelOpen: false,
  usagePanelOpen: false,
  settings: {
    locale: 'en-US',
    taskManager: true,
    cavemanMode: 'lite',
    usagePanel: true,
    auxiliaryEnabled: false,
    auxiliaryProviderId: '',
    auxiliaryModelId: '',
    providers: [],
  },
  providers: [],
  providerTemplates: [],
  sessions: [],
  graphLoading: false,
  features: {},
};

export const app = writable<AppState>(defaults);
export const activeSession = derived(app, ($app) =>
  $app.sessions.find((session) => session.id === $app.activeSessionId),
);

let unsubscribeRuntime: (() => void) | undefined;

function asString(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined;
}

function asNumber(value: unknown): number | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? value : undefined;
}

function sessionTitle(content: string): string {
  return content.trim().replace(/\s+/g, ' ').slice(0, 36) || 'session.new';
}

function updateSession(id: string, updater: (session: SessionSnapshot) => SessionSnapshot): void {
  app.update((state) => ({
    ...state,
    sessions: state.sessions.map((session) => (session.id === id ? updater(session) : session)),
  }));
}

function ensureAssistant(session: SessionSnapshot, messageId?: string): [SessionSnapshot, number] {
  const messages = [...session.messages];
  let index = messageId ? messages.findIndex((message) => message.id === messageId) : -1;
  if (index < 0) {
    const existing = messages[messages.length - 1];
    if (existing?.role === 'assistant' && session.status === 'streaming') index = messages.length - 1;
  }
  if (index < 0) {
    messages.push({ id: messageId ?? crypto.randomUUID(), role: 'assistant', content: '' });
    index = messages.length - 1;
  }
  return [{ ...session, messages }, index];
}

function mergeTask(data: Record<string, unknown>, previous?: TaskSnapshot): TaskSnapshot | undefined {
  const snapshot = data.task as TaskSnapshot | undefined;
  if (snapshot?.id) return snapshot;
  if (!previous && !asString(data.id)) return undefined;
  return {
    id: asString(data.id) ?? previous!.id,
    status: (asString(data.status) as TaskSnapshot['status']) ?? previous?.status ?? 'running',
    completed: asNumber(data.completed) ?? previous?.completed ?? 0,
    total: asNumber(data.total) ?? previous?.total ?? 0,
    current: asString(data.current) ?? previous?.current,
    items: (Array.isArray(data.items) ? data.items : previous?.items ?? []) as TaskSnapshot['items'],
  };
}

interface DeltaAccumulator {
  sessionId: string;
  messageId?: string;
  text: string;
  reasoning: Map<string, string>;
}

let deltaAccum: DeltaAccumulator | undefined;
let pendingEvents: RuntimeEvent[] = [];
let flushScheduled = false;

function flushDeltaAccumulator(): void {
  const pending = deltaAccum;
  deltaAccum = undefined;
  if (!pending || (!pending.text && pending.reasoning.size === 0)) return;
  const { sessionId, messageId, text, reasoning } = pending;
  updateSession(sessionId, (current) => {
    const [session, index] = ensureAssistant(current, messageId);
    const messages = [...session.messages];
    const message = messages[index];
    const next: MessageSnapshot = { ...message };
    if (text) next.content = message.content + text;
    if (reasoning.size) {
      const blocks = [...(message.reasoning ?? [])];
      for (const [blockId, delta] of reasoning) {
        const blockIndex = blocks.findIndex((block) => block.id === blockId);
        if (blockIndex < 0) blocks.push({ id: blockId, summary: delta, status: 'running' });
        else blocks[blockIndex] = { ...blocks[blockIndex], summary: blocks[blockIndex].summary + delta };
      }
      next.reasoning = blocks;
    }
    messages[index] = next;
    return { ...session, status: 'streaming', lastError: undefined, messages };
  });
}

function accumulateDelta(event: RuntimeEvent): boolean {
  if (event.kind !== 'assistant.text_delta' && event.kind !== 'assistant.reasoning_delta') return false;
  const sessionId = event.sessionId;
  if (!sessionId) return true;
  const messageId = asString(event.data.messageId);
  const blockId = asString(event.data.blockId) ?? 'reasoning';
  const delta = asString(event.data.delta) ?? '';
  if (!deltaAccum
    || deltaAccum.sessionId !== sessionId
    || (deltaAccum.messageId ?? undefined) !== (messageId ?? undefined)) {
    flushDeltaAccumulator();
    deltaAccum = { sessionId, messageId, text: '', reasoning: new Map() };
  }
  if (event.kind === 'assistant.text_delta') deltaAccum.text += delta;
  else deltaAccum.reasoning.set(blockId, (deltaAccum.reasoning.get(blockId) ?? '') + delta);
  return true;
}

function applyRuntimeEvent(event: RuntimeEvent): void {
  if (accumulateDelta(event)) return;
  flushDeltaAccumulator();
  applyNonDeltaEvent(event);
}

function scheduleEventFlush(): void {
  if (flushScheduled) return;
  flushScheduled = true;
  const run = (): void => {
    flushScheduled = false;
    const batch = pendingEvents;
    pendingEvents = [];
    for (const event of batch) applyRuntimeEvent(event);
    flushDeltaAccumulator();
  };
  if (typeof requestAnimationFrame === 'function') requestAnimationFrame(run);
  else queueMicrotask(run);
}

function handleRuntimeEvent(event: RuntimeEvent): void {
  // 高频流式 delta 与普通事件统一进帧队列：同帧内合并应用，避免每个
  // SSE token 都触发一次全量 store 更新与组件重渲染。
  pendingEvents.push(event);
  scheduleEventFlush();
}

function applyNonDeltaEvent(event: RuntimeEvent): void {
  const id = event.sessionId;
  const data = event.data;

  if (event.kind.startsWith('market.')) {
    handleMarketEvent(event);
    return;
  }

  // 内置浏览器被打开（用户或 agent）：切到浏览器页，让舞台上报矩形、
  // 子 webview 精确嵌入，而不是悬浮在当前页上像一个弹窗。
  if (event.kind === 'browser.opened' || event.kind === 'browser.updated') {
    const browser = data as unknown as BrowserSnapshot;
    app.update((state) => ({
      ...state,
      page: event.kind === 'browser.opened' ? 'browser' : state.page,
      browser: Array.isArray(browser.tabs)
        ? browser
        : { ...(state.browser ?? { available: true, open: false, tabs: [] }), ...browser },
    }));
    return;
  }

  // 插件预览按钮 → 原生 UI 页面切换（工坊经后端转发到主窗口）
  if (event.kind === 'ui.page') {
    const page = typeof data.page === 'string' ? data.page : '';
    if (page === 'studio') {
      void bridge.openPluginStudio().catch(() => undefined);
      return;
    }
    if (['workspace', 'settings', 'browser', 'market', 'graph'].includes(page)) {
      app.update((state) => ({ ...state, page: page as Page }));
    }
    return;
  }

  if (event.kind === 'provider.model_probe') {
    const models = data.models;
    if (Array.isArray(models)) app.update((state) => ({ ...state, providers: models as ProviderModel[] }));
    return;
  }

  if (event.kind === 'project.index_progress' || event.kind === 'project.ready' || event.kind === 'project.failed') {
    const project = (data.project ?? data) as ProjectSnapshot;
    app.update((state) => ({ ...state, project }));
    return;
  }

  if (!id) return;

  if (event.kind === 'session.deleted') {
    app.update((state) => {
      const sessions = state.sessions.filter((session) => session.id !== id);
      return { ...state, sessions, activeSessionId: state.activeSessionId === id ? sessions[0]?.id : state.activeSessionId };
    });
    return;
  }

  if (event.kind === 'session.renamed') {
    const snapshot = data.session as SessionSnapshot | undefined;
    if (snapshot?.id) updateSession(snapshot.id, (current) => ({ ...current, ...snapshot, messages: current.messages.length ? current.messages : snapshot.messages }));
    return;
  }

  if (event.kind === 'session.started') {
    const snapshot = data.session as SessionSnapshot | undefined;
    if (snapshot?.id) {
      app.update((state) => ({
        ...state,
        sessions: [
          (() => {
            const existing = state.sessions.find((session) => session.id === snapshot.id);
            return existing
              ? {
                  ...snapshot,
                  messages: existing.messages.length ? existing.messages : snapshot.messages,
                  task: existing.task ?? snapshot.task,
                  usage: existing.usage ?? snapshot.usage,
                }
              : snapshot;
          })(),
          ...state.sessions.filter((session) => session.id !== snapshot.id),
        ],
        activeSessionId: snapshot.id,
      }));
    }
    return;
  }

  updateSession(id, (current) => {
    if (event.kind.startsWith('auxiliary.') || event.kind === 'skill.activated') {
      const [session, index] = ensureAssistant(current, asString(data.messageId));
      const messages = [...session.messages];
      const message = messages[index];
      const auxiliary = [...(message.auxiliary ?? [])];
      const isSkill = event.kind === 'skill.activated';
      const role = isSkill ? 'skillRouter' : asString(data.role) ?? 'requirementReducer';
      const receiptId = asString(data.id) ?? `auxiliary:${asString(data.turnId) ?? event.eventId}:${role}`;
      const receiptIndex = auxiliary.findIndex((item) => item.id === receiptId);
      const previous = receiptIndex < 0 ? undefined : auxiliary[receiptIndex];
      const status = isSkill || event.kind === 'auxiliary.completed'
        ? 'completed'
        : event.kind === 'auxiliary.failed'
          ? 'failed'
          : event.kind === 'auxiliary.skipped' ? 'skipped' : 'running';
      const receipt = {
        id: receiptId,
        role,
        model: asString(data.model) ?? previous?.model,
        status,
        reason: asString(data.reason) ?? previous?.reason,
        beforeTokens: asNumber(data.beforeTokens) ?? previous?.beforeTokens,
        afterTokens: asNumber(data.afterTokens) ?? previous?.afterTokens,
        inputTokens: asNumber(data.inputTokens) ?? previous?.inputTokens,
        outputTokens: asNumber(data.outputTokens) ?? previous?.outputTokens,
        elapsedMs: asNumber(data.elapsedMs) ?? previous?.elapsedMs,
        summary: isSkill && Array.isArray(data.selected)
          ? data.selected.map((item) => {
              const selected = item as Record<string, unknown>;
              return `${asString(selected.name) ?? asString(selected.id) ?? 'skill'} · ${asString(selected.reason) ?? 'matched'}`;
            }).join('\n')
          : asString(data.summary) ?? previous?.summary,
      } as const;
      if (receiptIndex < 0) auxiliary.push(receipt);
      else auxiliary[receiptIndex] = receipt;
      messages[index] = { ...message, auxiliary };
      return { ...session, status: 'streaming', lastError: undefined, messages };
    }

    if (event.kind === 'assistant.tool_call' || event.kind.startsWith('tool.')) {
      const [session, index] = ensureAssistant(current, asString(data.messageId));
      const messages = [...session.messages];
      const message = messages[index];
      const tools = [...(message.tools ?? [])];
      const tool = (data.tool ?? data) as Partial<ToolReceipt>;
      const toolId = tool.id ?? asString(data.toolId) ?? asString(data.callId);
      if (!toolId) return current;
      const toolIndex = tools.findIndex((item) => item.id === toolId);
      const projection = (data.projection ?? {}) as Record<string, unknown>;
      const projectedStatus = asString(projection.status);
      const existing = toolIndex < 0 ? undefined : tools[toolIndex];
      const incomingArguments = tool.arguments ?? data.arguments;
      const toolArguments = incomingArguments && typeof incomingArguments === 'object' && !Array.isArray(incomingArguments)
        ? incomingArguments as Record<string, unknown>
        : existing?.arguments;
      const projectionSnapshot: ToolProjectionSnapshot = {
        status: projectedStatus,
        summary: asString(projection.summary),
        exitCode: asNumber(projection.exitCode ?? projection.exit_code),
        errorKey: asString(projection.errorKey ?? projection.error_key),
        completeness: asString(projection.completeness),
        total: asNumber(projection.total),
        truncated: typeof projection.truncated === 'boolean' ? projection.truncated : undefined,
        artifactId: asString(projection.artifactId ?? projection.artifact_id),
      };
      const receipt: ToolReceipt = {
        id: toolId,
        name: tool.name ?? asString(data.name) ?? existing?.name ?? toolId,
        status: tool.status ?? (projectedStatus === 'failed' ? 'failed' : projectedStatus === 'aborted' ? 'cancelled' : event.kind === 'tool.failed' ? 'failed' : event.kind === 'tool.completed' ? 'completed' : 'running'),
        arguments: toolArguments,
        elapsedMs: tool.elapsedMs ?? existing?.elapsedMs,
        summary: tool.summary ?? projectionSnapshot.summary ?? existing?.summary,
        diagnostic: tool.diagnostic ?? projectionSnapshot.errorKey ?? existing?.diagnostic,
        artifactId: tool.artifactId ?? projectionSnapshot.artifactId ?? existing?.artifactId,
        projection: {
          status: projectionSnapshot.status ?? existing?.projection?.status,
          summary: projectionSnapshot.summary ?? existing?.projection?.summary,
          exitCode: projectionSnapshot.exitCode ?? existing?.projection?.exitCode,
          errorKey: projectionSnapshot.errorKey ?? existing?.projection?.errorKey,
          completeness: projectionSnapshot.completeness ?? existing?.projection?.completeness,
          total: projectionSnapshot.total ?? existing?.projection?.total,
          truncated: projectionSnapshot.truncated ?? existing?.projection?.truncated,
          artifactId: projectionSnapshot.artifactId ?? existing?.projection?.artifactId,
        },
      };
      if (toolIndex < 0) tools.push(receipt);
      else tools[toolIndex] = { ...tools[toolIndex], ...receipt };
      messages[index] = { ...message, tools };
      return { ...session, status: 'streaming', lastError: undefined, messages };
    }

    if (event.kind === 'assistant.usage') {
      const raw = (data.usage ?? data) as Record<string, unknown>;
      const roundRaw = (data.roundUsage ?? data.round_usage ?? {}) as Record<string, unknown>;
      const promptDetails = (raw.prompt_tokens_details ?? raw.promptTokensDetails ?? {}) as Record<string, unknown>;
      const completionDetails = (raw.completion_tokens_details ?? raw.completionTokensDetails ?? {}) as Record<string, unknown>;
      const cacheRead = asNumber(raw.cache_read_tokens ?? raw.cacheReadTokens ?? raw.cached_input_tokens ?? raw.cachedInputTokens ?? promptDetails.cached_tokens ?? promptDetails.cachedTokens);
      const prompt = asNumber(raw.prompt_tokens ?? raw.promptTokens ?? raw.input_tokens ?? raw.inputTokens);
      const fresh = asNumber(data.freshInputTokens ?? raw.fresh_input_tokens ?? raw.freshInputTokens);
      const roundCacheRead = asNumber(roundRaw.cache_read_tokens ?? roundRaw.cacheReadTokens ?? roundRaw.cached_input_tokens ?? roundRaw.cachedInputTokens) ?? 0;
      const roundPrompt = asNumber(roundRaw.prompt_tokens ?? roundRaw.promptTokens ?? roundRaw.input_tokens ?? roundRaw.inputTokens);
      const roundFresh = asNumber(roundRaw.fresh_input_tokens ?? roundRaw.freshInputTokens)
        ?? (roundPrompt === undefined ? undefined : Math.max(0, roundPrompt - roundCacheRead));
      const cumulativeFresh = fresh ?? (prompt === undefined ? current.usage?.freshInputTokens : Math.max(0, prompt - (cacheRead ?? 0)));
      const cumulativeCacheRead = cacheRead ?? current.usage?.cacheReadTokens;
      const inferredRoundFresh = Math.max(0, (cumulativeFresh ?? 0) - (current.usage?.freshInputTokens ?? 0));
      const inferredRoundCache = Math.max(0, (cumulativeCacheRead ?? 0) - (current.usage?.cacheReadTokens ?? 0));
      const turnId = asString(data.turnId ?? data.turn_id);
      const continuesTurn = Boolean(turnId && current.usage?.turnId === turnId);
      const hasCurrentContext = Object.prototype.hasOwnProperty.call(raw, 'currentContextTokens')
        || Object.prototype.hasOwnProperty.call(raw, 'current_context_tokens');
      const usage: UsageSnapshot = {
        ...(current.usage ?? {}),
        turnId,
        turnFreshInputTokens: (continuesTurn ? current.usage?.turnFreshInputTokens ?? 0 : 0) + (roundFresh ?? inferredRoundFresh),
        turnCacheReadTokens: (continuesTurn ? current.usage?.turnCacheReadTokens ?? 0 : 0) + (roundRaw && Object.keys(roundRaw).length ? roundCacheRead : inferredRoundCache),
        freshInputTokens: cumulativeFresh,
        cacheReadTokens: cumulativeCacheRead,
        cacheWriteTokens: asNumber(raw.cache_write_tokens ?? raw.cacheWriteTokens) ?? current.usage?.cacheWriteTokens,
        outputTokens: asNumber(raw.completion_tokens ?? raw.completionTokens ?? raw.output_tokens ?? raw.outputTokens) ?? current.usage?.outputTokens,
        reasoningTokens: asNumber(raw.reasoning_tokens ?? raw.reasoningTokens ?? completionDetails.reasoning_tokens ?? completionDetails.reasoningTokens) ?? current.usage?.reasoningTokens,
        requestCount: asNumber(data.requestCount ?? raw.request_count ?? raw.requestCount) ?? current.usage?.requestCount,
        currentContextTokens: hasCurrentContext
          ? asNumber(raw.current_context_tokens ?? raw.currentContextTokens)
          : current.usage?.currentContextTokens,
      };
      return { ...current, usage };
    }

    if (event.kind === 'task.updated') {
      return { ...current, task: mergeTask(data, current.task) };
    }

    if (event.kind === 'assistant.completed') {
      return {
        ...current,
        status: 'idle',
        lastError: undefined,
        messages: current.messages.map((message) => ({
          ...message,
          reasoning: message.reasoning?.map((block) => ({ ...block, status: 'completed' })),
        })),
      };
    }

    if (event.kind === 'assistant.cancelled') return { ...current, status: 'idle', lastError: undefined };
    if (event.kind === 'assistant.failed') {
      const error = data.error as LocalizedError | undefined;
      return { ...current, status: 'failed', lastError: error?.key ? error : { key: 'conversation.failed' } };
    }
    return current;
  });
}

export async function bootstrap(): Promise<void> {
  app.update((state) => ({ ...state, runtime: 'connecting', error: undefined }));
  try {
    const payload = await bridge.bootstrap();
    app.update((state) => ({
      ...state,
      runtime: 'ready',
      settings: payload.settings,
      providers: payload.providers,
      providerTemplates: payload.providerTemplates,
      sessions: payload.sessions,
      activeSessionId: payload.activeSessionId,
      project: payload.project,
      browser: payload.browser,
      features: payload.features ?? {},
      usagePanelOpen: false,
    }));
    unsubscribeRuntime?.();
    unsubscribeRuntime = await bridge.subscribe(handleRuntimeEvent);
  } catch (cause) {
    const error = cause as LocalizedError;
    app.update((state) => ({
      ...state,
      runtime: 'offline',
      error: error?.key ? error : { key: 'runtime.offline' },
    }));
  }
}

export function setPage(page: Page): void {
  app.update((state) => ({ ...state, page }));
}

export function toggleSidebar(): void {
  app.update((state) => ({ ...state, sidebarCollapsed: !state.sidebarCollapsed }));
}

export function toggleTasks(): void {
  app.update((state) => ({ ...state, taskPanelOpen: !state.taskPanelOpen }));
}

export function toggleUsage(): void {
  app.update((state) => ({ ...state, usagePanelOpen: !state.usagePanelOpen }));
}

export function selectSession(id?: string): void {
  app.update((state) => ({ ...state, activeSessionId: id, page: 'workspace' }));
}

export async function createSession(): Promise<void> {
  const state = get(app);
  const selected = state.providers.find((model) =>
    model.available
    && model.providerId === state.settings.providerId
    && model.modelId === state.settings.modelId,
  ) ?? state.providers.find((model) => model.available);
  const session = await bridge.createSession({
    projectRoot: state.project?.root,
    provider: selected?.providerId,
    model: selected?.modelId,
  });
  app.update((current) => ({
    ...current,
    page: 'workspace',
    sessions: [session, ...current.sessions.filter((item) => item.id !== session.id)],
    activeSessionId: session.id,
  }));
}

export async function renameSession(id: string, title: string): Promise<void> {
  const snapshot = await bridge.renameSession(id, title);
  updateSession(id, (current) => ({ ...current, ...snapshot, messages: current.messages }));
}

export async function deleteSession(id: string): Promise<void> {
  await bridge.deleteSession(id);
  app.update((state) => {
    const sessions = state.sessions.filter((session) => session.id !== id);
    return {
      ...state,
      sessions,
      activeSessionId: state.activeSessionId === id ? sessions[0]?.id : state.activeSessionId,
      page: 'workspace',
    };
  });
}

export async function updateSettings(patch: Partial<SettingsSnapshot>): Promise<void> {
  const snapshot = await bridge.updateSettings(patch);
  const providers = snapshot.providers.flatMap((profile) => profile.models.map((model) => ({
    providerId: profile.id, providerName: profile.name, modelId: model.id, modelName: model.name || model.id,
    available: true, capabilities: model.capabilities, contextLimit: model.contextLimit,
    thinkingEnabled: model.thinkingEnabled ?? false, thinkingEffort: model.thinkingEffort ?? 'medium',
    thinkingToggle: model.thinkingToggle ?? false, thinkingEfforts: model.thinkingEfforts ?? [],
    adapter: model.adapter ?? profile.adapter,
  } satisfies ProviderModel)));
  // 后端返回值已完成校验并移除明文密钥；不得再用原始 patch 覆盖它。
  app.update((state) => ({ ...state, settings: snapshot, providers }));
}

export async function configureModelThinking(
  providerId: string,
  modelId: string,
  enabled: boolean,
  effort: ProviderModel['thinkingEffort'],
): Promise<void> {
  const state = get(app);
  const profiles = state.settings.providers.map((profile) => profile.id === providerId
    ? {
        ...profile,
        models: profile.models.map((model) => model.id === modelId
          ? { ...model, thinkingEnabled: enabled, thinkingEffort: effort }
          : model),
      }
    : profile);
  await updateSettings({ providers: profiles });
}

export async function selectModel(providerId: string, modelId: string): Promise<void> {
  const state = get(app);
  const selected = state.providers.find((model) =>
    model.available && model.providerId === providerId && model.modelId === modelId,
  );
  if (!selected) return;

  const session = state.sessions.find((item) => item.id === state.activeSessionId);
  if (session?.status === 'streaming') return;
  const settings = await bridge.updateSettings({ providerId, modelId });
  let updatedSession: SessionSnapshot | undefined;
  if (session) {
    updatedSession = await bridge.updateSessionModel(session.id, providerId, modelId);
  }
  app.update((current) => ({
    ...current,
    settings: { ...settings, providerId, modelId },
    sessions: updatedSession
      ? current.sessions.map((item) => item.id === updatedSession.id
        ? { ...item, ...updatedSession, messages: item.messages }
        : item)
      : current.sessions,
  }));
}

export async function submit(content: string, clarify = false, attachments: MessageAttachment[] = []): Promise<void> {
  const state = get(app);
  if (state.project?.status === 'indexing' || state.project?.status === 'updating') return;
  let sessionId = state.activeSessionId;
  const isGuidance = state.sessions.find((session) => session.id === sessionId)?.status === 'streaming';
  if (!sessionId) {
    await createSession();
    sessionId = get(app).activeSessionId;
  }
  if (!sessionId) return;
  updateSession(sessionId, (session) => ({
    ...session,
    status: 'streaming',
    lastError: undefined,
    title: session.messages.length === 0 ? sessionTitle(content) : session.title,
    usage: {
      ...(session.usage ?? {}),
      turnId: undefined,
      turnFreshInputTokens: undefined,
      turnCacheReadTokens: undefined,
    },
    messages: [
      ...session.messages,
      { id: crypto.randomUUID(), role: 'user', content, attachments } satisfies MessageSnapshot,
    ],
  }));
  try {
    await bridge.send(sessionId, content, clarify, attachments);
  } catch (cause) {
    const error = cause as LocalizedError;
    updateSession(sessionId, (session) => ({
      ...session,
      status: isGuidance ? session.status : 'failed',
      lastError: error?.key ? error : { key: 'conversation.failed' },
    }));
  }
}

export async function openProject(): Promise<void> {
  const snapshot = await bridge.openProject();
  if (!snapshot) return;
  app.update((state) => ({ ...state, project: snapshot, page: 'workspace' }));
}

export async function loadGraph(): Promise<void> {
  const root = get(app).project?.root;
  if (!root) {
    app.update((state) => ({ ...state, page: 'graph', graph: undefined, graphError: undefined }));
    return;
  }
  app.update((state) => ({ ...state, page: 'graph', graphLoading: true, graphError: undefined }));
  try {
    const graph = await bridge.projectGraph(root);
    app.update((state) => ({ ...state, graph, graphLoading: false, graphError: undefined }));
  } catch (cause) {
    const error = cause as LocalizedError;
    app.update((state) => ({ ...state, graphLoading: false, graphError: error?.key ? error : { key: 'error.project_not_indexed' } }));
  }
}

export async function stopActive(): Promise<void> {
  const sessionId = get(app).activeSessionId;
  if (sessionId) await bridge.stop(sessionId);
}

export async function runBrowserCommand(action: Parameters<typeof bridge.browserCommand>[0], url?: string, tabId?: string): Promise<void> {
  const snapshot = await bridge.browserCommand(action, url, tabId);
  app.update((state) => ({ ...state, browser: snapshot }));
}

export function destroy(): void {
  unsubscribeRuntime?.();
  unsubscribeRuntime = undefined;
}
