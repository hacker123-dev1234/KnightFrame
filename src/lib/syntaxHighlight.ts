import hljs from 'highlight.js/lib/core';
import bash from 'highlight.js/lib/languages/bash';
import css from 'highlight.js/lib/languages/css';
import java from 'highlight.js/lib/languages/java';
import javascript from 'highlight.js/lib/languages/javascript';
import json from 'highlight.js/lib/languages/json';
import kotlin from 'highlight.js/lib/languages/kotlin';
import powershell from 'highlight.js/lib/languages/powershell';
import python from 'highlight.js/lib/languages/python';
import rust from 'highlight.js/lib/languages/rust';
import typescript from 'highlight.js/lib/languages/typescript';
import xml from 'highlight.js/lib/languages/xml';

hljs.registerLanguage('bash', bash);
hljs.registerLanguage('css', css);
hljs.registerLanguage('java', java);
hljs.registerLanguage('javascript', javascript);
hljs.registerLanguage('json', json);
hljs.registerLanguage('kotlin', kotlin);
hljs.registerLanguage('powershell', powershell);
hljs.registerLanguage('python', python);
hljs.registerLanguage('rust', rust);
hljs.registerLanguage('typescript', typescript);
hljs.registerLanguage('xml', xml);

const languageAliases: Record<string, string> = {
  bash: 'bash', sh: 'bash', shell: 'bash', zsh: 'bash',
  css: 'css',
  html: 'xml', htm: 'xml', xml: 'xml', svg: 'xml',
  java: 'java',
  javascript: 'javascript', js: 'javascript', jsx: 'javascript',
  json: 'json', jsonc: 'json',
  kotlin: 'kotlin', kt: 'kotlin', kts: 'kotlin',
  powershell: 'powershell', ps1: 'powershell', pwsh: 'powershell',
  python: 'python', py: 'python',
  rust: 'rust', rs: 'rust',
  typescript: 'typescript', ts: 'typescript', tsx: 'typescript',
};

function escapeHtml(source: string): string {
  return source.replace(/[&<>"']/g, (character) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#039;',
  })[character] ?? character);
}

export interface HighlightedCode {
  html: string;
  language?: string;
}

export function highlightFencedCode(source: string, info?: string): HighlightedCode {
  const requested = info?.trim().split(/\s+/, 1)[0]?.toLowerCase() ?? '';
  const language = languageAliases[requested];
  return {
    html: language
      ? hljs.highlight(source, { language, ignoreIllegals: true }).value
      : escapeHtml(source),
    language,
  };
}
