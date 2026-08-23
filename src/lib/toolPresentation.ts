import { duration, number, translate } from './i18n';
import type { Locale, ReceiptStatus, ToolReceipt } from './types';

export interface ToolFact {
  label: string;
  value: string;
  mono?: boolean;
}

export interface ToolTextBlock {
  label: string;
  value: string;
  kind: 'content' | 'output' | 'error';
}

export interface ToolListItem {
  title: string;
  meta?: string;
  detail?: string;
}

export interface ToolList {
  label: string;
  items: ToolListItem[];
}

export interface ToolPresentation {
  name: string;
  summary: string;
  facts: ToolFact[];
  blocks: ToolTextBlock[];
  lists: ToolList[];
  truncated: boolean;
}

type JsonRecord = Record<string, unknown>;

const knownToolNames = new Set(['find', 'read', 'edit', 'run', 'search', 'web_search', 'web_fetch', 'browser', 'recall', 'skill', 'task']);
const textFields = new Set(['content', 'stdout', 'stderr', 'output', 'error', 'detail']);

function asRecord(value: unknown): JsonRecord | undefined {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as JsonRecord
    : undefined;
}

function parsePayload(raw?: string): unknown {
  if (!raw?.trim()) return undefined;
  let value: unknown = raw.trim();
  for (let attempt = 0; attempt < 2 && typeof value === 'string'; attempt += 1) {
    const text = value.trim();
    if (!text || !['{', '[', '"'].includes(text[0])) break;
    try {
      value = JSON.parse(text) as unknown;
    } catch {
      return undefined;
    }
  }
  return value;
}

function oneLine(value: unknown, limit = 120): string {
  const text = String(value ?? '').replace(/\r?\n/g, ' ').replace(/\s+/g, ' ').trim();
  if (text.length <= limit) return text;
  return `${text.slice(0, Math.max(0, limit - 1)).trimEnd()}…`;
}

function humanizeIdentifier(value: string): string {
  const spaced = value
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/[._-]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
  return spaced ? spaced[0].toLocaleUpperCase() + spaced.slice(1) : value;
}

function translatedField(locale: Locale, key: string): string {
  const translationKey = `tool.field.${key}`;
  const translated = translate(locale, translationKey);
  return translated === translationKey ? humanizeIdentifier(key) : translated;
}

function localizedValue(locale: Locale, value: string): string {
  const translated = translate(locale, value);
  return translated === value ? value : translated;
}

function scalar(locale: Locale, key: string, value: string | number | boolean): string {
  if (typeof value === 'boolean') return translate(locale, value ? 'tool.value.yes' : 'tool.value.no');
  if (typeof value === 'number') {
    if (key === 'elapsedMs' || key === 'durationMs') return duration(locale, value);
    return number(locale, value);
  }
  if ((key === 'status' || key === 'state') && value) {
    const translated = translate(locale, `tool.state.${value}`);
    if (translated !== `tool.state.${value}`) return translated;
  }
  if (['title', 'current', 'detail'].includes(key)) return localizedValue(locale, value);
  return value;
}

function fallbackSummary(locale: Locale, status: ReceiptStatus): string {
  return translate(locale, `tool.summary.${status}`);
}

function pathRange(locale: Locale, record: JsonRecord): string | undefined {
  const start = typeof record.startLine === 'number' ? record.startLine : undefined;
  const end = typeof record.endLine === 'number' ? record.endLine : undefined;
  if (start === undefined || end === undefined) return undefined;
  return translate(locale, 'tool.value.lines', { start, end });
}

function summarize(locale: Locale, tool: ToolReceipt, payload: unknown): string {
  if (Array.isArray(payload)) return translate(locale, 'tool.value.items', { count: payload.length });
  const record = asRecord(payload);
  if (!record) return typeof payload === 'string' && payload.trim()
    ? oneLine(payload)
    : fallbackSummary(locale, tool.status);

  const path = typeof record.path === 'string' ? oneLine(record.path, 82) : undefined;
  const range = pathRange(locale, record);
  if (path && range) return `${path} · ${range}`;

  if (path && typeof record.replacements === 'number') {
    return `${path} · ${translate(locale, 'tool.value.replacements', { count: record.replacements })}`;
  }

  const exitCode = typeof record.exitCode === 'number' ? record.exitCode : tool.projection?.exitCode;
  if (exitCode !== undefined) {
    const exit = exitCode === 0
      ? translate(locale, 'tool.value.exitSuccess')
      : translate(locale, 'tool.value.exitCode', { code: exitCode });
    const elapsed = typeof record.elapsedMs === 'number' ? ` · ${duration(locale, record.elapsedMs)}` : '';
    return `${exit}${elapsed}`;
  }

  if (Array.isArray(record.matches)) {
    const total = typeof record.total === 'number' ? record.total : record.matches.length;
    return translate(locale, 'tool.value.matches', { count: total });
  }

  if (typeof record.completed === 'number' && typeof record.total === 'number') {
    const progress = translate(locale, 'task.progress', { done: record.completed, total: record.total });
    return typeof record.current === 'string' && record.current.trim()
      ? `${progress} · ${oneLine(localizedValue(locale, record.current), 72)}`
      : progress;
  }

  const compact = Object.entries(record)
    .filter(([key, value]) => !textFields.has(key) && ['string', 'number', 'boolean'].includes(typeof value))
    .slice(0, 2)
    .map(([key, value]) => `${translatedField(locale, key)}: ${oneLine(scalar(locale, key, value as string | number | boolean), 64)}`);
  return compact.length ? compact.join(' · ') : fallbackSummary(locale, tool.status);
}

function listItem(locale: Locale, value: unknown, index: number): ToolListItem {
  const record = asRecord(value);
  if (!record) return { title: oneLine(value, 100) || `#${index + 1}` };

  const path = typeof record.path === 'string' ? record.path : undefined;
  const line = typeof record.line === 'number' ? record.line : undefined;
  const rawTitle = record.title ?? record.name ?? record.id ?? `#${index + 1}`;
  const title = path
    ? `${path}${line === undefined ? '' : `:${line}`}`
    : oneLine(typeof rawTitle === 'string' ? localizedValue(locale, rawTitle) : rawTitle, 100);
  const detailValue = record.text ?? record.content ?? record.summary ?? record.detail;
  const meta = Object.entries(record)
    .filter(([key, entry]) => !['path', 'line', 'title', 'name', 'id', 'text', 'content', 'summary', 'detail'].includes(key)
      && ['string', 'number', 'boolean'].includes(typeof entry))
    .slice(0, 3)
    .map(([key, entry]) => `${translatedField(locale, key)}: ${scalar(locale, key, entry as string | number | boolean)}`)
    .join(' · ');
  return {
    title,
    meta: meta || undefined,
    detail: detailValue === undefined
      ? undefined
      : typeof detailValue === 'string'
        ? localizedValue(locale, detailValue)
        : String(detailValue),
  };
}

function formatNestedValue(locale: Locale, value: unknown): string {
  if (Array.isArray(value)) return translate(locale, 'tool.value.items', { count: value.length });
  const record = asRecord(value);
  if (!record) return scalar(locale, '', value as string | number | boolean);
  const summary = Object.entries(record)
    .filter(([, entry]) => ['string', 'number', 'boolean'].includes(typeof entry))
    .slice(0, 3)
    .map(([key, entry]) => `${translatedField(locale, key)}: ${oneLine(scalar(locale, key, entry as string | number | boolean), 56)}`)
    .join(' · ');
  return summary || translate(locale, 'tool.value.items', { count: Object.keys(record).length });
}

function details(locale: Locale, payload: unknown): Pick<ToolPresentation, 'facts' | 'blocks' | 'lists'> {
  const facts: ToolFact[] = [];
  const blocks: ToolTextBlock[] = [];
  const lists: ToolList[] = [];
  const record = asRecord(payload);

  if (Array.isArray(payload)) {
    if (payload.length) lists.push({ label: translate(locale, 'tool.result'), items: payload.map((item, index) => listItem(locale, item, index)) });
    return { facts, blocks, lists };
  }

  if (!record) {
    if (typeof payload === 'string' && payload.trim()) {
      blocks.push({ label: translate(locale, 'tool.result'), value: payload, kind: 'content' });
    }
    return { facts, blocks, lists };
  }

  for (const [key, value] of Object.entries(record)) {
    if (value === undefined || value === null || value === '') continue;
    if (key === 'truncated' && value === false) continue;
    const label = translatedField(locale, key);
    if (Array.isArray(value)) {
      if (value.length) lists.push({ label, items: value.map((item, index) => listItem(locale, item, index)) });
      continue;
    }
    const nested = asRecord(value);
    if (nested) {
      const items = Object.entries(nested).map(([nestedKey, nestedEntry], index) => ({
        title: translatedField(locale, nestedKey),
        detail: formatNestedValue(locale, nestedEntry),
        meta: `#${index + 1}`,
      }));
      if (items.length) lists.push({ label, items });
      continue;
    }
    if (textFields.has(key) || (typeof value === 'string' && (value.includes('\n') || value.length > 140))) {
      blocks.push({
        label,
        value: String(value),
        kind: key === 'stderr' || key === 'error' ? 'error' : key === 'stdout' || key === 'output' ? 'output' : 'content',
      });
      continue;
    }
    facts.push({
      label,
      value: scalar(locale, key, value as string | number | boolean),
      mono: key === 'path' || key.toLocaleLowerCase().endsWith('id'),
    });
  }
  return { facts, blocks, lists };
}

export function displayToolName(locale: Locale, name: string): string {
  const normalized = name.trim().toLocaleLowerCase();
  return knownToolNames.has(normalized)
    ? translate(locale, `tool.name.${normalized}`)
    : humanizeIdentifier(name);
}

export function presentTool(locale: Locale, tool: ToolReceipt): ToolPresentation {
  const payload = parsePayload(tool.summary);
  const structured = details(locale, payload);
  return {
    name: displayToolName(locale, tool.name),
    summary: summarize(locale, tool, payload),
    ...structured,
    truncated: tool.projection?.truncated === true,
  };
}
