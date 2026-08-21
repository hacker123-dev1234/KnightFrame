const port = Number(process.argv[2] ?? 9333);
const expression = process.argv[3] ?? `JSON.stringify({
  url: location.href,
  title: document.title,
  readyState: document.readyState,
  bootstrap: window.__KF_BOOTSTRAP__ ?? null,
  appHtml: document.querySelector('#app')?.innerHTML ?? null,
  bodyText: document.body?.innerText ?? null,
})`;

const targets = await fetch(`http://127.0.0.1:${port}/json`).then((response) => response.json());

async function evaluate(target) {
  const socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve, { once: true });
    socket.addEventListener('error', reject, { once: true });
  });
  const id = 1;
  const result = await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error(`CDP timeout: ${target.id}`)), 5_000);
    socket.addEventListener('message', (event) => {
      const message = JSON.parse(String(event.data));
      if (message.id !== id) return;
      clearTimeout(timeout);
      resolve(message);
    });
    socket.send(JSON.stringify({
      id,
      method: 'Runtime.evaluate',
      params: { expression, awaitPromise: true, returnByValue: true },
    }));
  });
  socket.close();
  return result;
}

const pages = [];
for (const target of targets.filter((item) => item.type === 'page')) {
  pages.push({
    id: target.id,
    title: target.title,
    url: target.url,
    evaluation: await evaluate(target),
  });
}
process.stdout.write(`${JSON.stringify(pages, null, 2)}\n`);
