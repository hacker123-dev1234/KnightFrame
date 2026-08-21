import DOMPurify from 'dompurify';
import { marked, Renderer } from 'marked';
import { highlightFencedCode } from './syntaxHighlight';

const renderer = new Renderer();
renderer.code = ({ text, lang }) => {
  const highlighted = highlightFencedCode(text, lang);
  const languageClass = highlighted.language ? ` language-${highlighted.language}` : '';
  return `<pre><code class="hljs${languageClass}">${highlighted.html}</code></pre>\n`;
};

marked.setOptions({
  async: false,
  breaks: true,
  gfm: true,
  renderer,
});

export function renderMarkdown(source: string): string {
  const rendered = marked.parse(source) as string;
  const safe = DOMPurify.sanitize(rendered, {
    USE_PROFILES: { html: true },
  });
  const document = new DOMParser().parseFromString(safe, 'text/html');
  for (const link of document.querySelectorAll('a')) {
    link.setAttribute('target', '_blank');
    link.setAttribute('rel', 'noreferrer noopener');
  }
  return document.body.innerHTML;
}
