import type { ReceiptStatus, SessionSnapshot, ToolReceipt } from './types';

const EDIT_NAMES = new Set(['edit', 'edit_file', 'multi_edit']);
const PATH_KEYS = ['path', 'filePath', 'file_path'];
const OLD_KEYS = ['oldText', 'old_text', 'oldString', 'old_string'];
const NEW_KEYS = ['newText', 'new_text', 'newString', 'new_string'];

function textValue(record: Record<string, unknown> | undefined, keys: string[]): string | undefined {
  for (const key of keys) {
    const value = record?.[key];
    if (typeof value === 'string') return value;
  }
  return undefined;
}

function lines(text: string): string[] {
  return text === '' ? [] : text.replace(/\r\n/g, '\n').split('\n');
}

export function changedLineCounts(oldText: string, newText: string): { additions: number; deletions: number } {
  const before = lines(oldText);
  const after = lines(newText);
  let prefix = 0;
  while (prefix < before.length && prefix < after.length && before[prefix] === after[prefix]) prefix += 1;
  let suffix = 0;
  while (
    suffix < before.length - prefix
    && suffix < after.length - prefix
    && before[before.length - 1 - suffix] === after[after.length - 1 - suffix]
  ) suffix += 1;
  return {
    additions: Math.max(0, after.length - prefix - suffix),
    deletions: Math.max(0, before.length - prefix - suffix),
  };
}

function editPath(tool: ToolReceipt): string | undefined {
  const fromArguments = textValue(tool.arguments, PATH_KEYS)?.trim();
  if (fromArguments) return fromArguments;
  const fromSummary = tool.summary?.match(/^Edited\s+(.+):\s+\d+\s+replacement/i)?.[1]?.trim();
  return fromSummary || undefined;
}

function currentTurnTools(session: SessionSnapshot): ToolReceipt[] {
  let lastUser = -1;
  session.messages.forEach((message, index) => { if (message.role === 'user') lastUser = index; });
  return session.messages.slice(lastUser + 1).flatMap((message) => message.tools ?? []);
}

export interface TaskCapsuleSummary {
  step: number;
  total: number;
  current: string;
  status: ReceiptStatus;
  filesChanged?: number;
  additions?: number;
  deletions?: number;
}

export function taskCapsuleSummary(session: SessionSnapshot): TaskCapsuleSummary | undefined {
  const task = session.task;
  if (!task || task.total <= 0) return undefined;
  // 会话创建时的默认单步自动任务（"处理当前请求"）不算真实任务：
  // 短问答不弹任务胶囊；只有 agent 真正用 task 工具扩充过的任务才显示。
  if (task.items.length === 1 && task.items[0].title === 'task.session') return undefined;
  const currentItem = task.items.find((item) => item.status === 'running')
    ?? task.items.find((item) => item.status === 'pending')
    ?? task.items[Math.min(task.items.length - 1, Math.max(0, task.completed))];
  const currentIndex = currentItem ? task.items.findIndex((item) => item.id === currentItem.id) : -1;
  const step = task.status === 'completed'
    ? task.total
    : Math.min(task.total, Math.max(1, currentIndex >= 0 ? currentIndex + 1 : task.completed + 1));
  const current = task.current || currentItem?.title || 'task.current';

  const edits = currentTurnTools(session).filter((tool) => EDIT_NAMES.has(tool.name.trim().toLowerCase()) && tool.status === 'completed');
  const paths = edits.map(editPath);
  const hasCompletePaths = edits.length > 0 && paths.every(Boolean);
  const uniquePaths = new Set(paths.filter((path) => path !== undefined));
  let additions = 0;
  let deletions = 0;
  let completeLineStats = edits.length > 0;
  for (const edit of edits) {
    const oldText = textValue(edit.arguments, OLD_KEYS);
    const newText = textValue(edit.arguments, NEW_KEYS);
    if (oldText === undefined || newText === undefined) {
      completeLineStats = false;
      break;
    }
    const changed = changedLineCounts(oldText, newText);
    additions += changed.additions;
    deletions += changed.deletions;
  }

  return {
    step,
    total: task.total,
    current,
    status: task.status,
    filesChanged: hasCompletePaths ? uniquePaths.size : undefined,
    additions: completeLineStats ? additions : undefined,
    deletions: completeLineStats ? deletions : undefined,
  };
}
