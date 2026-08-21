import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import net from 'node:net';
import path from 'node:path';
import process from 'node:process';

const projectRoot = path.resolve(import.meta.dirname, '..');
const executable = path.resolve(process.argv[2] ?? path.join(projectRoot, 'KnightFrame-Test.exe'));
const targetProject = path.resolve(process.argv[3] ?? path.join(projectRoot, '..', 'agent-standalone'));
const outputDirectory = path.join(projectRoot, 'src-tauri', 'target', 'e2e');
const fixturePath = path.join(targetProject, 'KNIGHTFRAME_E2E_FIXTURE.txt');
const turnTimeoutMs = Number(process.env.KF_E2E_TURN_TIMEOUT_MS ?? 360_000);

if (!existsSync(executable)) throw new Error(`Executable not found: ${executable}`);
if (!existsSync(targetProject)) throw new Error(`Target project not found: ${targetProject}`);

const delay = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

async function freePort() {
  return await new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      server.close(() => resolve(address.port));
    });
  });
}

async function waitForTarget(port, child, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`KnightFrame exited during startup: ${child.exitCode}`);
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json`);
      const targets = await response.json();
      const target = targets.find((item) => item.type === 'page' && item.webSocketDebuggerUrl);
      if (target) return target;
    } catch {
      // WebView2 has not opened the debug endpoint yet.
    }
    await delay(200);
  }
  throw new Error('Timed out waiting for the WebView2 debug target');
}

class CdpClient {
  constructor(url) {
    this.url = url;
    this.nextId = 1;
    this.pending = new Map();
  }

  async connect() {
    this.socket = new WebSocket(this.url);
    this.socket.addEventListener('message', (event) => {
      const message = JSON.parse(event.data);
      if (!message.id) return;
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      if (message.error) pending.reject(new Error(`${pending.method}: ${message.error.message}`));
      else pending.resolve(message.result);
    });
    await new Promise((resolve, reject) => {
      this.socket.addEventListener('open', resolve, { once: true });
      this.socket.addEventListener('error', reject, { once: true });
    });
    await this.send('Runtime.enable');
    await this.send('Page.enable');
  }

  send(method, params = {}) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { method, resolve, reject });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }

  async evaluate(expression) {
    const result = await this.send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (result.exceptionDetails) {
      const detail = result.exceptionDetails.exception?.description
        ?? result.exceptionDetails.text
        ?? 'JavaScript evaluation failed';
      throw new Error(detail);
    }
    return result.result.value;
  }

  async screenshot(fileName) {
    const result = await this.send('Page.captureScreenshot', {
      format: 'png',
      fromSurface: true,
      captureBeyondViewport: false,
    });
    await writeFile(path.join(outputDirectory, fileName), Buffer.from(result.data, 'base64'));
  }

  close() {
    this.socket?.close();
  }
}

async function waitUntil(client, expression, timeoutMs, label) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await client.evaluate(expression)) return;
    await delay(200);
  }
  throw new Error(`Timed out waiting for ${label}`);
}

function js(value) {
  return JSON.stringify(value);
}

async function invoke(client, command, args = {}) {
  return await client.evaluate(`window.__TAURI_INTERNALS__.invoke(${js(command)}, ${js(args)})`);
}

async function setTextAndClick(client, content, selector) {
  await client.evaluate(`(() => {
    const textarea = document.querySelector('textarea');
    if (!(textarea instanceof HTMLTextAreaElement)) throw new Error('Composer textarea not found');
    const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set;
    setter.call(textarea, ${js(content)});
    textarea.dispatchEvent(new Event('input', { bubbles: true }));
    const button = document.querySelector(${js(selector)});
    if (!(button instanceof HTMLButtonElement) || button.disabled) throw new Error('Composer action unavailable');
    button.click();
  })()`);
}

async function runTurn(client, content) {
  await setTextAndClick(client, content, '.composer .send-button:not(.stop):not(.guide)');
  await waitUntil(client, `Boolean(document.querySelector('.composer .send-button.stop'))`, 15_000, 'streaming to start');
  await waitUntil(client, `!document.querySelector('.composer .send-button.stop')`, turnTimeoutMs, 'streaming to finish');
  await delay(250);
  return await client.evaluate(`(() => ({
    failed: Boolean(document.querySelector('.conversation-error')),
    error: document.querySelector('.conversation-error')?.textContent?.trim() ?? '',
    answer: [...document.querySelectorAll('.message.assistant .message-content')].at(-1)?.textContent?.trim() ?? '',
  }))()`);
}

async function selectModel(client, key) {
  await client.evaluate(`(() => {
    const select = document.querySelector('.model-chip select');
    if (!(select instanceof HTMLSelectElement)) throw new Error('Model selector not found');
    select.value = ${js(key)};
    select.dispatchEvent(new Event('change', { bubbles: true }));
  })()`);
  await waitUntil(
    client,
    `document.querySelector('.model-chip select')?.value === ${js(key)} && !document.querySelector('.model-chip select')?.disabled`,
    15_000,
    `model selection ${key}`,
  );
}

async function createSession(client) {
  const previous = await client.evaluate(`document.querySelectorAll('.session-row').length`);
  await client.evaluate(`document.querySelector('.new-button')?.click()`);
  await waitUntil(
    client,
    `document.querySelectorAll('.session-row').length > ${previous} && Boolean(document.querySelector('.session-row.active'))`,
    15_000,
    'new session',
  );
}

async function toolState(client) {
  return await client.evaluate(`(() => {
    const cards = [...document.querySelectorAll('.tool-card')];
    return {
      count: cards.length,
      bodies: document.querySelectorAll('.tool-card .tool-body').length,
      allFolded: cards.every((card) => card.querySelector('.tool-heading')?.getAttribute('aria-expanded') !== 'true'),
      cards: cards.map((card) => ({
        text: card.textContent?.replace(/\\s+/g, ' ').trim() ?? '',
        failed: card.classList.contains('failed'),
        running: card.classList.contains('running'),
      })),
    };
  })()`);
}

async function expandToolCards(client) {
  await client.evaluate(`document.querySelectorAll('.tool-heading[aria-expanded="false"]').forEach((button) => button.click())`);
  await delay(350);
  return await toolState(client);
}

async function graphFocusSmoke(client) {
  const opened = await client.evaluate(`(() => {
    const labels = new Set(['Project graph', '项目图谱']);
    const button = [...document.querySelectorAll('.sidebar-bottom button')]
      .find((item) => labels.has(item.getAttribute('aria-label')));
    button?.click();
    return Boolean(button);
  })()`);
  if (!opened) throw new Error('Project graph navigation was not found');
  await waitUntil(client, `Boolean(document.querySelector('.graph-canvas'))`, 30_000, 'project graph');
  await client.screenshot('graph-overview.png');
  await client.evaluate(`(() => {
    const canvas = document.querySelector('.graph-canvas');
    canvas.focus();
    canvas.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  })()`);
  await waitUntil(client, `document.querySelector('.graph-canvas-host')?.classList.contains('focused')`, 5_000, 'graph focus');
  await delay(650);
  await client.screenshot('graph-focused.png');
  await client.evaluate(`document.querySelector('.graph-canvas')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))`);
  await waitUntil(client, `!document.querySelector('.graph-canvas-host')?.classList.contains('focused')`, 5_000, 'graph focus exit');
}

async function autoScrollSmoke(client) {
  const prepare = `(() => {
    const stream = document.querySelector('.conversation-stream');
    const feed = document.querySelector('.conversation-feed');
    if (!(stream instanceof HTMLElement) || !(feed instanceof HTMLElement)) return false;
    stream.scrollTop = stream.scrollHeight;
    stream.dispatchEvent(new Event('scroll'));
    return true;
  })()`;
  if (!await client.evaluate(prepare)) throw new Error('Conversation stream was not found');
  await delay(100);

  await client.evaluate(`(() => {
    const fixture = document.createElement('div');
    fixture.className = 'e2e-scroll-fixture';
    fixture.style.cssText = 'height:720px;min-height:720px;width:1px;flex:none;pointer-events:none';
    document.querySelector('.conversation-feed')?.append(fixture);
  })()`);
  await delay(250);
  const followedAtBottom = await client.evaluate(`(() => {
    const stream = document.querySelector('.conversation-stream');
    return stream instanceof HTMLElement
      && stream.scrollHeight - stream.clientHeight - stream.scrollTop <= 2;
  })()`);

  const detachedTop = await client.evaluate(`(() => {
    const stream = document.querySelector('.conversation-stream');
    if (!(stream instanceof HTMLElement)) return -1;
    stream.scrollTop = Math.max(0, stream.scrollHeight - stream.clientHeight - 240);
    stream.dispatchEvent(new Event('scroll'));
    return stream.scrollTop;
  })()`);
  await client.evaluate(`(() => {
    const fixture = document.createElement('div');
    fixture.className = 'e2e-scroll-fixture';
    fixture.style.cssText = 'height:520px;min-height:520px;width:1px;flex:none;pointer-events:none';
    document.querySelector('.conversation-feed')?.append(fixture);
  })()`);
  await delay(250);
  const stayedDetached = await client.evaluate(`(() => {
    const stream = document.querySelector('.conversation-stream');
    return stream instanceof HTMLElement && Math.abs(stream.scrollTop - ${js(detachedTop)}) <= 2;
  })()`);

  await client.evaluate(`(() => {
    const stream = document.querySelector('.conversation-stream');
    if (!(stream instanceof HTMLElement)) return;
    stream.scrollTop = stream.scrollHeight;
    stream.dispatchEvent(new Event('scroll'));
  })()`);
  await delay(100);
  await client.evaluate(`(() => {
    const fixture = document.createElement('div');
    fixture.className = 'e2e-scroll-fixture';
    fixture.style.cssText = 'height:360px;min-height:360px;width:1px;flex:none;pointer-events:none';
    document.querySelector('.conversation-feed')?.append(fixture);
  })()`);
  await delay(250);
  const resumedAtBottom = await client.evaluate(`(() => {
    const stream = document.querySelector('.conversation-stream');
    return stream instanceof HTMLElement
      && stream.scrollHeight - stream.clientHeight - stream.scrollTop <= 2;
  })()`);
  await client.evaluate(`(() => {
    document.querySelectorAll('.e2e-scroll-fixture').forEach((fixture) => fixture.remove());
    const stream = document.querySelector('.conversation-stream');
    if (stream instanceof HTMLElement) stream.scrollTop = stream.scrollHeight;
  })()`);
  return { followedAtBottom, stayedDetached, resumedAtBottom };
}

async function longTaskAndRecovery(client) {
  const prompt = '请用 run 执行命令：Start-Sleep -Seconds 3; Write-Output KF_LONG_OK。完成后报告输出。';
  await setTextAndClick(client, prompt, '.composer .send-button:not(.stop):not(.guide)');
  await waitUntil(client, `Boolean(document.querySelector('.composer .send-button.stop'))`, 15_000, 'long task start');
  await waitUntil(client, `Boolean(document.querySelector('.tool-card.running'))`, 90_000, 'long run tool');
  await setTextAndClick(client, '中途指导：完成后同时说明命令退出码。', '.composer .send-button.guide');
  await waitUntil(client, `!document.querySelector('.composer .send-button.stop')`, turnTimeoutMs, 'guided long task finish');
  const guidedText = await client.evaluate(`[...document.querySelectorAll('.message.assistant .message-content')].at(-1)?.textContent ?? ''`);

  await setTextAndClick(
    client,
    '请用 run 执行命令：Start-Sleep -Seconds 20; Write-Output SHOULD_NOT_FINISH。',
    '.composer .send-button:not(.stop):not(.guide)',
  );
  await waitUntil(client, `Boolean(document.querySelector('.tool-card.running'))`, 90_000, 'cancellable run tool');
  await client.evaluate(`document.querySelector('.composer .send-button.stop')?.click()`);
  await waitUntil(client, `!document.querySelector('.composer .send-button.stop')`, 20_000, 'cancellation');
  const recovery = await runTurn(client, '请只回复 CONTINUE_OK。');
  return {
    guidanceApplied: guidedText.includes('KF_LONG_OK'),
    cancellationRecovered: !recovery.failed && recovery.answer.includes('CONTINUE_OK'),
  };
}

await mkdir(outputDirectory, { recursive: true });
const fixtureExisted = existsSync(fixturePath);
const originalFixture = fixtureExisted ? await readFile(fixturePath) : undefined;
const debugPort = await freePort();
const child = spawn(executable, [], {
  cwd: projectRoot,
  env: {
    ...process.env,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort}`,
  },
  stdio: 'ignore',
  windowsHide: true,
});

let client;
const report = {
  executable,
  targetProject,
  startedAt: new Date().toISOString(),
  models: [],
};

try {
  const target = await waitForTarget(debugPort, child);
  client = new CdpClient(target.webSocketDebuggerUrl);
  await client.connect();
  await waitUntil(
    client,
    `document.readyState === 'complete' && Boolean(document.querySelector('.model-chip select')) && !document.querySelector('.runtime-banner')`,
    30_000,
    'KnightFrame bootstrap',
  );

  await writeFile(fixturePath, 'MODEL_PENDING\n', 'utf8');
  await invoke(client, 'kf_project_open', { path: targetProject });
  await delay(300);

  const catalog = await client.evaluate(`(() => {
    const select = document.querySelector('.model-chip select');
    return [...select.options].map((option) => ({ key: option.value, name: option.textContent.trim() }));
  })()`);
  if (!catalog.length) throw new Error('The compatibility endpoint returned no available free models');
  report.catalog = catalog;

  for (let index = 0; index < catalog.length; index += 1) {
    const model = catalog[index];
    await writeFile(fixturePath, 'MODEL_PENDING\n', 'utf8');
    await invoke(client, 'kf_project_open', { path: targetProject });
    await selectModel(client, model.key);
    await createSession(client);

    const overview = await runTurn(
      client,
      '请说明当前打开的项目是什么，主要技术栈和核心模块是什么。请用项目中的证据简短回答。',
    );
    const expectedMarker = `MODEL_OK_${index + 1}`;
    const modification = await runTurn(
      client,
      `找到当前项目中唯一包含 MODEL_PENDING 的文件，将它精确替换为 ${expectedMarker}，验证后简短回答，不要改其他文件。`,
    );
    const fixture = (await readFile(fixturePath, 'utf8')).trim();
    const folded = await toolState(client);
    if (index === 0) await client.screenshot('tool-cards-folded.png');
    const expanded = await expandToolCards(client);
    if (index === 0) await client.screenshot('tool-cards-expanded.png');
    const toolText = expanded.cards.map((card) => card.text).join('\n');
    const enumerationAttempted = /directory enumeration|目录枚举|Get-ChildItem|\b(?:ls|dir|tree)\b/i.test(toolText)
      && expanded.cards.some((card) => card.failed);
    const rawJsonVisible = /(?:^|\s)[{[]\s*"(?:matches|content|stdout|exitCode)"\s*:/m.test(toolText);
    const usedIndexedSearch = /Search text|搜索文本|Find in project|项目内查找/.test(toolText);
    const usedEdit = /Edit file|编辑文件/.test(toolText);

    const result = {
      ...model,
      overview,
      modification,
      fixture,
      expectedMarker,
      foldedByDefault: folded.allFolded && folded.bodies === 0,
      enumerationAttempted,
      rawJsonVisible,
      usedIndexedSearch,
      usedEdit,
      tools: expanded.cards,
      passed: !overview.failed
        && overview.answer.length > 0
        && !modification.failed
        && fixture === expectedMarker
        && folded.allFolded
        && folded.bodies === 0
        && !enumerationAttempted
        && !rawJsonVisible
        && usedIndexedSearch
        && usedEdit,
    };
    report.models.push(result);
    await writeFile(path.join(outputDirectory, 'free-models.json'), `${JSON.stringify(report, null, 2)}\n`);
    await delay(1_500);
  }

  report.longTask = await longTaskAndRecovery(client);
  report.autoScroll = await autoScrollSmoke(client);
  await graphFocusSmoke(client);
  report.graphFocus = true;
  report.passed = report.models.every((model) => model.passed)
    && report.longTask.guidanceApplied
    && report.longTask.cancellationRecovered
    && report.autoScroll.followedAtBottom
    && report.autoScroll.stayedDetached
    && report.autoScroll.resumedAtBottom
    && report.graphFocus;
  report.finishedAt = new Date().toISOString();
  await writeFile(path.join(outputDirectory, 'free-models.json'), `${JSON.stringify(report, null, 2)}\n`);
  if (!report.passed) process.exitCode = 1;
} finally {
  client?.close();
  child.kill();
  if (fixtureExisted) await writeFile(fixturePath, originalFixture);
  else await rm(fixturePath, { force: true });
}

console.log(JSON.stringify(report, null, 2));
