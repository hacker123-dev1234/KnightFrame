import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { createServer as createTcpServer } from 'node:net';
import { access, mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, extname, join, resolve, sep } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const dist = join(root, 'dist');
const artifacts = join(root, 'artifacts');
const edgeCandidates = [
  process.env['ProgramFiles(x86)'] && join(process.env['ProgramFiles(x86)'], 'Microsoft', 'Edge', 'Application', 'msedge.exe'),
  process.env.PROGRAMFILES && join(process.env.PROGRAMFILES, 'Microsoft', 'Edge', 'Application', 'msedge.exe'),
  process.env.LOCALAPPDATA && join(process.env.LOCALAPPDATA, 'Microsoft', 'Edge', 'Application', 'msedge.exe'),
].filter(Boolean);

const mime = {
  '.css': 'text/css; charset=utf-8',
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.png': 'image/png',
};

async function firstExisting(paths) {
  for (const path of paths) {
    try {
      await access(path);
      return path;
    } catch {}
  }
  throw new Error('Microsoft Edge was not found');
}

async function unusedPort() {
  const server = createTcpServer();
  await new Promise((resolveListen, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolveListen);
  });
  const address = server.address();
  const port = typeof address === 'object' && address ? address.port : 0;
  await new Promise((resolveClose) => server.close(resolveClose));
  return port;
}

function staticServer() {
  return createServer(async (request, response) => {
    try {
      const url = new URL(request.url ?? '/', 'http://127.0.0.1');
      const pathname = decodeURIComponent(url.pathname === '/' ? '/index.html' : url.pathname);
      const path = resolve(dist, `.${pathname}`);
      if (path !== dist && !path.startsWith(`${dist}${sep}`)) {
        response.writeHead(403).end();
        return;
      }
      const content = await readFile(path);
      response.writeHead(200, { 'content-type': mime[extname(path)] ?? 'application/octet-stream' });
      response.end(content);
    } catch {
      response.writeHead(404).end();
    }
  });
}

async function waitForDebugger(port) {
  const deadline = Date.now() + 10_000;
  while (Date.now() < deadline) {
    try {
      const targets = await fetch(`http://127.0.0.1:${port}/json`).then((response) => response.json());
      const page = targets.find((target) => target.type === 'page');
      if (page) return page;
    } catch {}
    await new Promise((resolveWait) => setTimeout(resolveWait, 80));
  }
  throw new Error('Edge DevTools endpoint did not become ready');
}

async function connectCdp(url) {
  const socket = new WebSocket(url);
  await new Promise((resolveOpen, reject) => {
    socket.addEventListener('open', resolveOpen, { once: true });
    socket.addEventListener('error', reject, { once: true });
  });
  let nextId = 0;
  const pending = new Map();
  const listeners = new Map();
  socket.addEventListener('message', (event) => {
    const message = JSON.parse(String(event.data));
    if (message.id) {
      const waiter = pending.get(message.id);
      if (!waiter) return;
      pending.delete(message.id);
      if (message.error) waiter.reject(new Error(message.error.message));
      else waiter.resolve(message.result);
      return;
    }
    for (const listener of listeners.get(message.method) ?? []) listener(message.params);
  });
  return {
    close: () => socket.close(),
    on(method, listener) {
      listeners.set(method, [...(listeners.get(method) ?? []), listener]);
    },
    send(method, params = {}) {
      const id = ++nextId;
      return new Promise((resolveCommand, reject) => {
        pending.set(id, { resolve: resolveCommand, reject });
        socket.send(JSON.stringify({ id, method, params }));
      });
    },
    waitFor(method, timeout = 8_000) {
      return new Promise((resolveEvent, reject) => {
        const timer = setTimeout(() => reject(new Error(`CDP event timeout: ${method}`)), timeout);
        const listener = (params) => {
          clearTimeout(timer);
          listeners.set(method, (listeners.get(method) ?? []).filter((item) => item !== listener));
          resolveEvent(params);
        };
        listeners.set(method, [...(listeners.get(method) ?? []), listener]);
      });
    },
  };
}

async function navigate(cdp, url) {
  const loaded = cdp.waitFor('Page.loadEventFired');
  await cdp.send('Page.navigate', { url });
  await loaded;
  await new Promise((resolveWait) => setTimeout(resolveWait, 180));
}

async function evaluate(cdp, expression) {
  const result = await cdp.send('Runtime.evaluate', {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
  return result.result?.value;
}

async function screenshot(cdp, name) {
  const capture = await cdp.send('Page.captureScreenshot', { format: 'png', fromSurface: true });
  const path = join(artifacts, name);
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, Buffer.from(capture.data, 'base64'));
  return path;
}

function intersects(a, b) {
  return a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top;
}

const server = staticServer();
const profile = await mkdtemp(join(tmpdir(), 'knightframe-headless-'));
let edge;
let cdp;
try {
  await new Promise((resolveListen, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolveListen);
  });
  const address = server.address();
  const webPort = typeof address === 'object' && address ? address.port : 0;
  const debugPort = await unusedPort();
  const edgePath = await firstExisting(edgeCandidates);
  edge = spawn(edgePath, [
    '--headless=new',
    '--disable-gpu',
    '--hide-scrollbars',
    '--no-first-run',
    '--no-default-browser-check',
    '--window-size=1280,780',
    `--remote-debugging-port=${debugPort}`,
    `--user-data-dir=${profile}`,
    'about:blank',
  ], { stdio: 'ignore', windowsHide: true });

  const target = await waitForDebugger(debugPort);
  cdp = await connectCdp(target.webSocketDebuggerUrl);
  const exceptions = [];
  cdp.on('Runtime.exceptionThrown', ({ exceptionDetails }) => exceptions.push(exceptionDetails.text));
  await cdp.send('Page.enable');
  await cdp.send('Runtime.enable');

  await navigate(cdp, `http://127.0.0.1:${webPort}/studio.html`);
  const studio = await evaluate(cdp, `(async () => {
    const palette = [...document.querySelectorAll('.studio-palette-list button')];
    palette[0]?.click();
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const shell = document.querySelector('.studio-shell');
    const canvas = document.querySelector('.studio-canvas');
    return {
      title: document.title,
      shell: Boolean(shell),
      palette: palette.length,
      nodes: document.querySelectorAll('.studio-node').length,
      count: document.querySelector('.studio-layers .studio-panel-heading small')?.textContent?.trim(),
      canvas: canvas ? { width: canvas.getBoundingClientRect().width, height: canvas.getBoundingClientRect().height } : null,
      overflow: document.body.scrollWidth > innerWidth || document.body.scrollHeight > innerHeight,
    };
  })()`);
  if (!studio.shell || studio.palette < 7 || studio.nodes !== 1 || studio.count !== '1') {
    throw new Error(`Plugin Studio interaction failed: ${JSON.stringify(studio)}`);
  }
  if (!studio.canvas || studio.canvas.width < 500 || studio.canvas.height < 300 || studio.overflow) {
    throw new Error(`Plugin Studio layout failed: ${JSON.stringify(studio)}`);
  }
  const studioShot = await screenshot(cdp, 'plugin-studio-headless.png');

  const studioCode = await evaluate(cdp, `(async () => {
    const tabs = [...document.querySelectorAll('.studio-view-tabs button')];
    tabs[1]?.click();
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const files = [...document.querySelectorAll('.studio-code-files > button')];
    const editor = document.querySelector('.studio-editor > textarea');
    const apply = document.querySelector('.studio-editor header button');
    let parsed = false;
    let appliedName = '';
    if (editor && !editor.readOnly) {
      const layout = JSON.parse(editor.value);
      parsed = Array.isArray(layout.ui) && layout.ui.length === 1;
      layout.name = 'Smoke Test Plugin';
      editor.value = JSON.stringify(layout, null, 2);
      editor.dispatchEvent(new Event('input', { bubbles: true }));
      apply?.click();
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      appliedName = JSON.parse(editor.value).name;
    }
    return {
      view: Boolean(document.querySelector('.studio-code-workspace')),
      files: files.length,
      fileNames: files.map((button) => button.textContent?.trim()),
      editable: Boolean(editor && !editor.readOnly),
      apply: Boolean(apply),
      parsed,
      appliedName,
      overflow: document.body.scrollWidth > innerWidth || document.body.scrollHeight > innerHeight,
    };
  })()`);
  if (!studioCode.view || studioCode.files !== 7 || !studioCode.editable || !studioCode.apply || !studioCode.parsed || studioCode.appliedName !== 'Smoke Test Plugin' || studioCode.overflow) {
    throw new Error(`Plugin Studio code view failed: ${JSON.stringify(studioCode)}`);
  }
  const studioCodeShot = await screenshot(cdp, 'plugin-studio-code-headless.png');

  const studioAssistant = await evaluate(cdp, `(async () => {
    const tabs = [...document.querySelectorAll('.studio-view-tabs button')];
    tabs[2]?.click();
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const textarea = document.querySelector('.studio-assistant-form textarea');
    const send = document.querySelector('.studio-assistant-form button[type="submit"]');
    const initiallyDisabled = send?.disabled;
    if (textarea) {
      textarea.value = 'Add one precise command button.';
      textarea.dispatchEvent(new Event('input', { bubbles: true }));
      await new Promise((resolve) => requestAnimationFrame(resolve));
    }
    return {
      view: Boolean(document.querySelector('.studio-assistant-workspace')),
      contextName: document.querySelector('.studio-assistant-context h2')?.textContent?.trim(),
      textarea: Boolean(textarea),
      send: Boolean(send),
      initiallyDisabled,
      enabledAfterInput: send ? !send.disabled : false,
      overflow: document.body.scrollWidth > innerWidth || document.body.scrollHeight > innerHeight,
    };
  })()`);
  if (!studioAssistant.view || studioAssistant.contextName !== 'Smoke Test Plugin' || !studioAssistant.textarea || !studioAssistant.send || !studioAssistant.initiallyDisabled || !studioAssistant.enabledAfterInput || studioAssistant.overflow) {
    throw new Error(`Plugin Studio assistant view failed: ${JSON.stringify(studioAssistant)}`);
  }
  const studioAssistantShot = await screenshot(cdp, 'plugin-studio-assistant-headless.png');

  const studioReturn = await evaluate(cdp, `(async () => {
    const tabs = [...document.querySelectorAll('.studio-view-tabs button')];
    tabs[0]?.click();
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const frame = document.querySelector('.studio-host-frame');
    const deadline = Date.now() + 5000;
    while (frame && frame.contentDocument?.readyState !== 'complete' && Date.now() < deadline) {
      await new Promise((resolve) => setTimeout(resolve, 50));
    }
    const frameDocument = frame?.contentDocument;
    return {
      design: Boolean(document.querySelector('.studio-workspace')),
      frame: Boolean(frame),
      frameUrl: frame?.contentWindow?.location.href,
      frameReady: frameDocument?.readyState,
      knightFrameLoaded: Boolean(frameDocument?.querySelector('.app-shell .main-workspace')),
      frameTitle: frameDocument?.title,
      nodeCount: document.querySelectorAll('.studio-node').length,
      frameNodeCount: frameDocument?.querySelectorAll('.kf-preview-node').length ?? 0,
    };
  })()`);
  if (!studioReturn.design || !studioReturn.frame || studioReturn.frameReady !== 'complete' || !studioReturn.knightFrameLoaded || !studioReturn.frameUrl?.includes('index.html?studioPreview=1') || studioReturn.nodeCount !== 1 || studioReturn.frameNodeCount !== 0) {
    throw new Error(`Plugin Studio did not return to a live KnightFrame host: ${JSON.stringify(studioReturn)}`);
  }

  // 工坊/预览双模式：预览=宿主可交互（iframe pointer-events 恢复 + 工坊编辑层隐藏 + 宿主覆盖层承接组件）
  const studioModes = await evaluate(cdp, `(async () => {
    const style = getComputedStyle(document.querySelector('.studio-host-frame'));
    const workshopLayer = document.querySelector('.studio-node-layer');
    const workshopPointer = style.pointerEvents;
    const toggle = [...document.querySelectorAll('.studio-mode-toggle button')].find((button) => button.textContent.includes('预览') || button.textContent.includes('Preview'));
    toggle?.click();
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const frameStyle = getComputedStyle(document.querySelector('.studio-host-frame'));
    const layerStyle = getComputedStyle(document.querySelector('.studio-node-layer'));
    const previewPointer = frameStyle.pointerEvents;
    const layerHidden = layerStyle.display === 'none';
    const frameNodes = document.querySelector('.studio-host-frame')?.contentDocument?.querySelectorAll('.kf-preview-node')?.length ?? 0;
    const workshop = [...document.querySelectorAll('.studio-mode-toggle button')].find((button) => button.textContent.includes('工坊') || button.textContent.includes('Workshop'));
    workshop?.click();
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const returnedLayerStyle = getComputedStyle(document.querySelector('.studio-node-layer'));
    const returnedFrameNodes = document.querySelector('.studio-host-frame')?.contentDocument?.querySelectorAll('.kf-preview-node')?.length ?? 0;
    return {
      workshopPointer,
      toggle: Boolean(toggle),
      previewPointer,
      layerHidden,
      frameNodes,
      returnedLayerVisible: returnedLayerStyle.display !== 'none',
      returnedFrameNodes,
    };
  })()`);
  if (studioModes.workshopPointer !== 'none' || !studioModes.toggle || studioModes.previewPointer !== 'auto' || !studioModes.layerHidden || studioModes.frameNodes < 1 || !studioModes.returnedLayerVisible || studioModes.returnedFrameNodes !== 0) {
    throw new Error(`Plugin Studio workshop/preview modes failed: ${JSON.stringify(studioModes)}`);
  }

  await navigate(cdp, `http://127.0.0.1:${webPort}/index.html`);
  const browserUi = await evaluate(cdp, `(async () => {
    const entry = document.querySelector('button[title="Browser"], button[title="浏览器"]');
    entry?.click();
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const page = document.querySelector('.browser-page');
    const tabs = document.querySelector('.browser-tabs');
    const toolbar = document.querySelector('.browser-toolbar');
    const stage = document.querySelector('.browser-stage');
    const dock = document.querySelector('.browser-dock');
    const resizer = document.querySelector('.browser-dock-resizer');
    const conversation = document.querySelector('.conversation-pane');
    const beforeWidth = dock?.getBoundingClientRect().width ?? 0;
    if (resizer) {
      const x = resizer.getBoundingClientRect().left;
      resizer.dispatchEvent(new PointerEvent('pointerdown', { bubbles:true, button:0, clientX:x }));
      window.dispatchEvent(new PointerEvent('pointermove', { bubbles:true, clientX:x + 60 }));
      window.dispatchEvent(new PointerEvent('pointerup', { bubbles:true, clientX:x + 60 }));
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    }
    const draggedWidth = dock?.getBoundingClientRect().width ?? 0;
    resizer?.dispatchEvent(new MouseEvent('dblclick', { bubbles:true }));
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const restoredWidth = dock?.getBoundingClientRect().width ?? 0;
    return {
      entry: Boolean(entry),
      page: Boolean(page),
      tabs: Boolean(tabs),
      toolbar: Boolean(toolbar),
      dock: Boolean(dock),
      resizer: Boolean(resizer),
      conversation: Boolean(conversation),
      beforeWidth,
      draggedWidth,
      restoredWidth,
      stage: stage ? { width: stage.getBoundingClientRect().width, height: stage.getBoundingClientRect().height } : null,
      overflow: page ? page.scrollWidth > page.clientWidth || page.scrollHeight > page.clientHeight : true,
    };
  })()`);
  if (!browserUi.entry || !browserUi.page || !browserUi.tabs || !browserUi.toolbar || !browserUi.dock || !browserUi.resizer || !browserUi.conversation || !browserUi.stage || browserUi.stage.width < 260 || browserUi.stage.height < 400 || browserUi.draggedWidth <= browserUi.beforeWidth || Math.abs(browserUi.restoredWidth - 420) > 2 || browserUi.overflow) {
    throw new Error(`Built-in browser shell failed: ${JSON.stringify(browserUi)}`);
  }
  const browserShot = await screenshot(cdp, 'browser-headless.png');
  await evaluate(cdp, `(async () => {
    document.querySelector('.browser-exit')?.click();
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  })()`);
  const browserRestore = await evaluate(cdp, `(async () => {
    const entry = document.querySelector('button[title="Browser"], button[title="浏览器"]');
    const closed = !document.querySelector('.browser-dock');
    entry?.click();
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const restored = Boolean(document.querySelector('.browser-dock'));
    document.querySelector('.browser-exit')?.click();
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    return { closed, restored };
  })()`);
  if (!browserRestore.closed || !browserRestore.restored) throw new Error(`Built-in browser restore failed: ${JSON.stringify(browserRestore)}`);
  const auxiliarySettings = await evaluate(cdp, `(async () => {
    const entry = document.querySelector('button[title="Settings"], button[title="设置"]');
    entry?.click();
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const choices = [...document.querySelectorAll('.auxiliary-source-picker button')];
    const localActive = choices[0]?.classList.contains('active');
    choices[1]?.click();
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const form = document.querySelector('.auxiliary-endpoint-form');
    const password = form?.querySelector('input[type="password"]');
    const modelInput = [...(form?.querySelectorAll('input') ?? [])].find((input) => input.placeholder?.includes('model') || input.placeholder?.includes('模型'));
    return {
      page: Boolean(document.querySelector('.settings-page')),
      choices: choices.length,
      localActive,
      networkActive: choices[1]?.classList.contains('active'),
      password: Boolean(password),
      modelInput: Boolean(modelInput),
      overflow: form ? form.scrollWidth > form.clientWidth : true,
    };
  })()`);
  if (!auxiliarySettings.page || auxiliarySettings.choices !== 2 || !auxiliarySettings.localActive || !auxiliarySettings.networkActive || !auxiliarySettings.password || !auxiliarySettings.modelInput || auxiliarySettings.overflow) {
    throw new Error(`Auxiliary model configuration is incomplete: ${JSON.stringify(auxiliarySettings)}`);
  }
  const settingsShot = await screenshot(cdp, 'settings-auxiliary-headless.png');
  await evaluate(cdp, `(async () => {
    document.querySelector('.back-link')?.click();
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  })()`);
  const main = await evaluate(cdp, `(() => {
    const box = (selector) => {
      const rect = document.querySelector(selector)?.getBoundingClientRect();
      return rect ? { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom } : null;
    };
    const fixture = document.createElement('div');
    fixture.className = 'model-popover';
    fixture.style.cssText = 'position:fixed;top:80px;right:20px;visibility:hidden';
    fixture.innerHTML = '<button><span class="model-option-indicator"></span><span><strong>A verified model</strong><small>Free provider subtitle</small></span><span></span></button>';
    document.body.append(fixture);
    const modelButton = fixture.querySelector('button').getBoundingClientRect();
    const modelSubtitle = fixture.querySelector('small').getBoundingClientRect();
    const indexing = document.createElement('section');
    indexing.className = 'indexing-gate';
    indexing.innerHTML = '<div class="indexing-field" aria-hidden="true"><div class="indexing-orbit orbit-outer">' + Array.from({ length: 8 }, (_, index) => '<i style="--angle:' + index * 45 + 'deg"></i>').join('') + '</div><div class="indexing-orbit orbit-inner">' + Array.from({ length: 5 }, (_, index) => '<i style="--angle:' + index * 72 + 'deg"></i>').join('') + '</div><div class="indexing-sword"><img src="/brand/knightframe-sword-gate-ui.png" alt=""></div><div class="indexing-knight"><img src="/brand/knightframe-ui-hero-white.png" alt=""></div><span class="indexing-strike"></span></div><div class="indexing-copy"><small>KnightFrame</small><h2>Forging the project index</h2><p>Mapping files, symbols, and their connections.</p><span class="indexing-pulse"><i></i><i></i><i></i></span></div>';
    document.querySelector('.main-workspace')?.append(indexing);
    const gate = indexing.getBoundingClientRect();
    const gatePointerEvents = getComputedStyle(indexing).pointerEvents;
    fixture.remove();
    return {
      composer: box('.composer'),
      context: box('.context-usage-hud'),
      cache: box('.cache-hit-hud'),
      metrics: box('.workspace-metrics'),
      workspace: box('.main-workspace'),
      cacheCopy: document.querySelector('.cache-hit-hud')?.textContent?.trim(),
      modelSubtitleFits: modelSubtitle.top >= modelButton.top && modelSubtitle.bottom <= modelButton.bottom,
      gate: { width: gate.width, height: gate.height, pointerEvents: gatePointerEvents },
    };
  })()`);
  if (!main.composer || !main.context || !main.cache || !main.metrics) {
    throw new Error(`Main workspace layout is incomplete: ${JSON.stringify(main)}`);
  }
  if (intersects(main.composer, main.context) || intersects(main.composer, main.cache)) {
    throw new Error(`Usage metrics overlap the composer: ${JSON.stringify(main)}`);
  }
  if (!main.modelSubtitleFits) throw new Error(`Model provider subtitle is clipped: ${JSON.stringify(main)}`);
  if (!main.cacheCopy || !/Session|会话/.test(main.cacheCopy)) {
    throw new Error(`Cache HUD does not distinguish current and session usage: ${JSON.stringify(main)}`);
  }
  const workspaceWidth = main.workspace?.right - main.workspace?.left;
  const workspaceHeight = main.workspace?.bottom - main.workspace?.top;
  if (!workspaceWidth || !workspaceHeight || main.gate.width < workspaceWidth - 1 || main.gate.height < workspaceHeight - 1 || main.gate.pointerEvents === 'none') {
    throw new Error(`Indexing gate does not lock the workspace: ${JSON.stringify(main)}`);
  }
  await evaluate(cdp, `new Promise((resolve) => setTimeout(resolve, 900))`);
  const indexingShot = await screenshot(cdp, 'indexing-gate-headless.png');
  await evaluate(cdp, `document.querySelector('.indexing-gate')?.remove()`);
  const mainShot = await screenshot(cdp, 'main-workspace-headless.png');

  if (exceptions.length) throw new Error(`Runtime exceptions: ${exceptions.join('; ')}`);
  process.stdout.write(`${JSON.stringify({ studio, studioCode, studioAssistant, studioReturn, browserUi, auxiliarySettings, main, screenshots: [studioShot, studioCodeShot, studioAssistantShot, browserShot, settingsShot, indexingShot, mainShot] }, null, 2)}\n`);
} finally {
  cdp?.close();
  edge?.kill();
  await new Promise((resolveWait) => setTimeout(resolveWait, 150));
  await new Promise((resolveClose) => server.close(resolveClose));
  await rm(profile, { recursive: true, force: true });
}
