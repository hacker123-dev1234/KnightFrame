import type { AnalysisRecord } from './types';

export interface TraceNode {
  nodeId: string;
  question: string;
  answer: string;
  reason: string;
  branch?: string;
  barRange?: string;
  skipped: boolean;
  section?: string;
  depth: number;
}

/** 从阶段一诊断 JSON 提取 decision_trace（含层级深度）。 */
export function extractTrace(record: AnalysisRecord | undefined): TraceNode[] {
  const trace = record?.stage1Diagnosis as Record<string, unknown> | undefined;
  const rows = trace?.decision_trace;
  if (!Array.isArray(rows)) return [];
  return rows
    .filter((row): row is Record<string, unknown> => typeof row === 'object' && row !== null)
    .map((row) => {
      const nodeId = typeof row.node_id === 'string' ? row.node_id : '';
      return {
        nodeId,
        question: typeof row.question === 'string' ? row.question : '',
        answer: typeof row.answer === 'string' ? row.answer : '',
        reason: typeof row.reason === 'string' ? row.reason : '',
        branch: typeof row.branch === 'string' ? row.branch : undefined,
        barRange: typeof row.bar_range === 'string' ? row.bar_range : undefined,
        skipped: row.skipped === true,
        section: typeof row.section === 'string' ? row.section : undefined,
        depth: nodeId.split('.').filter(Boolean).length - 1,
      };
    });
}

/** 从记录提取展示键值对（浅层）。 */
export function flattenFields(value: Record<string, unknown> | undefined, skip: string[] = []): [string, string][] {
  if (!value) return [];
  const pairs: [string, string][] = [];
  for (const [key, raw] of Object.entries(value)) {
    if (skip.includes(key)) continue;
    if (raw === null || raw === undefined) continue;
    if (typeof raw === 'object') continue;
    pairs.push([key, String(raw)]);
  }
  return pairs;
}
