// 本地 mock OpenAI 兼容 SSE 服务器：用于无头端到端测试，隔离外网。
// 行为：第 1 轮返回 read 工具调用（读 README.md + src/main.rs），
// 第 2 轮返回 edit 工具调用（修改 todo.txt），第 3 轮返回最终文本。
import http from 'node:http';

let turn = 0;
const sse = (res, chunks) => {
  res.writeHead(200, {
    'content-type': 'text/event-stream',
    'cache-control': 'no-cache',
    connection: 'keep-alive',
  });
  let delay = 8;
  for (const payload of chunks) {
    setTimeout(() => res.write(`data: ${JSON.stringify(payload)}\n\n`), delay);
    delay += 8;
  }
  setTimeout(() => {
    res.write('data: [DONE]\n\n');
    res.end();
  }, delay + 8);
};

const delta = (d) => ({ choices: [{ delta: d }] });
const finish = (reason) => ({ choices: [{ delta: {}, finish_reason: reason }] });

const server = http.createServer((req, res) => {
  if (!req.url.endsWith('/chat/completions')) {
    res.writeHead(404).end();
    return;
  }
  turn += 1;
  if (turn === 1) {
    sse(res, [
      delta({ reasoning_content: 'Inspecting the project first.' }),
      delta({
        tool_calls: [{
          index: 0,
          id: 'call-read-1',
          function: { name: 'read', arguments: '{"ranges":[{"path":"README.md"},{"path":"src/main.rs"}]}' },
        }],
      }),
      finish('tool_calls'),
    ]);
  } else if (turn === 2) {
    sse(res, [
      delta({ reasoning_content: 'Now applying the requested edit.' }),
      delta({
        tool_calls: [{
          index: 0,
          id: 'call-edit-1',
          function: { name: 'edit', arguments: '{"path":"todo.txt","oldText":"hello","newText":"world"}' },
        }],
      }),
      finish('tool_calls'),
    ]);
  } else {
    sse(res, [
      delta({ reasoning_content: 'Wrapping up.' }),
      delta({ content: '项目解释：这是一个 Rust 计算器演示，入口 src/main.rs 计算 2+3 并打印。已将 todo.txt 中的 hello 改为 world，edit 返回 1 处替换。' }),
      finish('stop'),
    ]);
  }
});

server.listen(8787, '127.0.0.1', () => console.log('mock-llm listening on 127.0.0.1:8787'));
