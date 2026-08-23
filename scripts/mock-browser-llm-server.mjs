// Headless Browser regression fixture: model calls Browser fetch, receives
// visible page text, then proves it could answer from that text.
import http from 'node:http';

const port = Number(process.env.KF_BROWSER_MOCK_PORT ?? 8788);
let modelTurns = 0;

function sse(response, payloads) {
  response.writeHead(200, {
    'content-type': 'text/event-stream',
    'cache-control': 'no-cache',
    connection: 'close',
  });
  for (const payload of payloads) response.write(`data: ${JSON.stringify(payload)}\n\n`);
  response.end('data: [DONE]\n\n');
}

const delta = (value) => ({ choices: [{ delta: value }] });
const finish = (reason) => ({ choices: [{ delta: {}, finish_reason: reason }] });

const server = http.createServer((request, response) => {
  if (request.url?.startsWith('/search')) {
    const body = '<html><head><title>Search results</title></head><body><a href="/profile">Psychiatry Director Guo</a><p>Hospital profile and clinic schedule.</p></body></html>';
    response.writeHead(200, {
      'content-type': 'text/html; charset=utf-8',
      'content-length': Buffer.byteLength(body),
      connection: 'close',
    });
    response.end(body);
    return;
  }
  if (!request.url?.endsWith('/chat/completions')) {
    response.writeHead(404).end();
    return;
  }
  let body = '';
  request.setEncoding('utf8');
  request.on('data', (chunk) => { body += chunk; });
  request.on('end', () => {
    modelTurns += 1;
    if (modelTurns === 1) {
      sse(response, [
        delta({ tool_calls: [{
          index: 0,
          id: 'browser-fetch-1',
          function: {
            name: 'browser',
            arguments: JSON.stringify({ action: 'fetch', url: `http://127.0.0.1:${port}/search?q=doctor` }),
          },
        }] }),
        finish('tool_calls'),
      ]);
      return;
    }
    let visible = false;
    try {
      const payload = JSON.parse(body);
      visible = payload.messages?.some((message) => message.role === 'tool'
        && String(message.content).includes('Psychiatry Director Guo')) === true;
    } catch {
      visible = false;
    }
    sse(response, [
      delta({ content: visible
        ? 'BROWSER_RESULT_OK: Psychiatry Director Guo; Hospital profile and clinic schedule.'
        : 'BROWSER_RESULT_MISSING' }),
      finish('stop'),
    ]);
  });
});

server.listen(port, '127.0.0.1', () => process.stdout.write(`ready:${port}\n`));
