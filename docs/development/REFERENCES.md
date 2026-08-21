# 参考、证据与许可证边界

## 1. 使用原则

KnightFrame 采用 clean-room 设计：提炼公开协议、架构模式、失败教训和可验证行为，不复制反编译 prompt、隐藏常量或无清晰许可证的实现。

参考证据分级：

```text
A  官方公开 API/协议文档
B  有许可证的本地开源源码与文档
C  本地旧 KnightFrame 的行为和问题
D  反编译/逆向项目、宣传 benchmark、未复现实验
```

- A 可作为 wire contract，但实现前仍需检查日期和 capability；
- B 可借鉴模式，若复制实质代码必须履行许可证和 NOTICE；
- C 只用于迁移语义和反模式；
- D 只能形成调查假设，不能作为实现真相或宣传证据。

## 2. 本地参考

### 2.1 OpenAI Codex Rust

- 路径：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\codex-main\codex-main`
- 许可证：Apache-2.0，见根 `LICENSE`
- 参考：Rust crate 边界、事件循环、工具 registry/executor、审批与 sandbox 分层、apply patch、上下文增量和输出截断。
- 不照搬：大 shell schema、行级整片 patch、产品内部 prompt/策略、任何未经公开说明的隐藏行为。

### 2.2 Claude Code 本地副本

- 路径：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\claude-code-2.7.1\claude-code-2.7.1`
- 证据：包和 README 明确自称 reverse-engineered/decompiled；根目录未发现 LICENSE。
- 用途：仅作为黑盒行为线索和兼容 fixture 线索。
- 禁止：复制源码、prompt、内部常量、分类器、缓存分块和反编译结构。
- 权威来源：Anthropic 官方公开文档。

### 2.3 Reasonix

- 路径：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\DeepSeek-Reasonix-main-v2\DeepSeek-Reasonix-main-v2`
- 许可证：MIT，见 `LICENSE`
- 参考：稳定 system/tools、append-only 会话、工具 schema 规范化、动态状态尾注入、usage 归一化。
- 已知缺口：部分冷输入成本、compaction 调用和 cache-write 价格处理存在漏算风险；不能照抄。

### 2.4 codebase-memory-mcp 0.9.0

- 路径：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\参考\codebase-memory-mcp-0.9.0\codebase-memory-mcp-0.9.0`
- 许可证：MIT，Copyright (c) 2025 DeusData。
- 参考：全量本地图谱、tree-sitter/LSP 混合、SQLite 持久化、增量 watcher、丰富节点与关系、架构/路径/变更查询。
- 不照搬：以宣传的 158 语言能力作为 KnightFrame 完成声明；一口气暴露全部工具；未经本项目复现的 token 数字。

### 2.5 Headroom

- 路径：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\参考\headroom-main\headroom-main`
- 许可证：Apache-2.0，包含 `LICENSE` 和 `NOTICE`。
- 参考：零 LLM 内容感知压缩、Raw/投影分离、live zone、只有变短才采用、artifact 回取。
- 不照搬：固定阈值、代理栈、未经复现的 60–95%/15–20% 宣传数字。

### 2.6 code-review-graph 2.3.7

- 路径：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\code-review-graph-main\code-review-graph-main`
- 许可证：MIT，Copyright (c) 2026 Tirth Kanani。
- 参考：符号/关系 schema、impact 方向与衰减、增量模式、最小 context、FTS 与歧义处理、独立评测意识。
- 重要反例：`OVERRIDES` 被列入 schema/评分但解析器没有生产；65x 是整库读取上界比较；部分独立 co-change/search/flow 指标并不理想。不得作为 KnightFrame 成绩。

### 2.7 OpenSquilla 0.5.3

- 路径：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\opensquilla-main`
- 许可证：Apache-2.0；另有 `THIRD_PARTY_NOTICES.md`。Tokenjuice 相关部分为 MIT 改编，见其 `PROVENANCE.md`/`LICENSE.tokenjuice`。
- 参考：副模型绑定物理 provider、主任务保留槽、Task runtime 状态模式、Turn 固定 Skill catalog、词法/语义混合路由、工具原始/投影双视图、i18n key parity。
- 不照搬：巨型 Python TaskRuntime/Agent、60,000 字符默认投影、固定 keepalive、正则全项目 source symbols、默认遥测和硬件派生 ID、多套分裂文案、PinchBench 宣传口径。

### 2.8 旧 KnightFrame / LexSilva

- 主要路径：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\agent-standalone`
- 参考：九层名称、用户认可的产品语义、历史 UI/工具/记忆/评测问题。
- 新工程不复制旧代码、状态、记忆库、provider 配置或缓存。
- 九层保留 L0 感知、L1 情节、L2 语义、L3 模式、L4 原则、L5 方法、L6 规则、L7 公理、L8 准则；16 维坐标删除。

## 3. 官方在线资料

本设计冻结于 2026-08-13。实现和发布前必须重新核验版本化能力。

### 3.1 OpenAI

- [Responses API streaming reference](https://platform.openai.com/docs/api-reference/responses-streaming)
- [Prompt caching guide](https://platform.openai.com/docs/guides/prompt-caching)

使用范围：Responses 事件、工具调用、reasoning、usage、prompt cache key/retention。具体模型支持的 effort、上下文和工具能力必须实时探测，不能从文档示例泛化。

### 3.2 Anthropic

- [Messages API](https://platform.claude.com/docs/en/api/messages)
- [Streaming Messages](https://platform.claude.com/docs/en/build-with-claude/streaming)
- [Prompt caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching)
- [Tool use with prompt caching](https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-use-with-prompt-caching)
- [Tools reference](https://code.claude.com/docs/en/tools-reference)
- [Permissions](https://code.claude.com/docs/en/permissions)
- [Hooks](https://code.claude.com/docs/en/hooks)
- [Skills](https://code.claude.com/docs/en/slash-commands)
- [Subagents](https://code.claude.com/docs/en/sub-agents)

使用范围：content block/SSE、tool JSON 累积、缓存顺序 `tools -> system -> messages`、cache usage、延迟工具、权限和 Skill 可观察行为。

### 3.3 DeepSeek

- [Create Chat Completion](https://api-docs.deepseek.com/api/create-chat-completion)
- [Thinking Mode](https://api-docs.deepseek.com/guides/thinking_mode)
- [Tool Calls](https://api-docs.deepseek.com/guides/tool_calls)
- [Context Caching](https://api-docs.deepseek.com/guides/kv_cache)
- [Models & Pricing](https://api-docs.deepseek.com/quick_start/pricing)

截至冻结日，文档公开了 OpenAI/Anthropic 格式入口、新模型、thinking、工具调用和 `prompt_cache_hit_tokens`/`prompt_cache_miss_tokens`。这些高度易变，必须用 adapter fixture + canary 锁定，不把当前 model ID/价格永久写入代码。

### 3.4 OpenRouter

- [Provider routing](https://openrouter.ai/docs/guides/routing/provider-selection)
- [Prompt caching and sticky routing](https://openrouter.ai/docs/guides/best-practices/prompt-caching)
- [Tool calling](https://openrouter.ai/docs/guides/features/tool-calling)
- [Provider logging/privacy](https://openrouter.ai/docs/guides/privacy/provider-logging/)

使用范围：物理 provider 选择、session stickiness、fallback、参数能力、数据策略、工具调用和缓存计量。Beta response caching 默认不用于主 agent loop。

### 3.5 MCP

- [Model Context Protocol specification](https://modelcontextprotocol.io/specification)

使用范围：工具 schema、structured content、resources、annotations 与错误。annotations 只作提示，KnightFrame 本地权限是最终边界。

## 4. 许可证流程

开发中必须维护 `THIRD_PARTY_NOTICES` 与 SBOM：

1. 只借鉴模式、不复制表达时，记录 inspiration 和本 clean-room 文档；
2. 复制或实质改写 Apache-2.0 代码时，保留许可证、归属、NOTICE 和修改说明；
3. 复制或实质改写 MIT 代码/规则时，保留版权与许可文本；
4. 翻译语料、图标、规则 JSON 和测试 fixture 同样可能受许可证约束；
5. 未发现明确许可证的本地项目不复制；
6. 第三方模型、grammar、二进制资产单独登记来源、版本、许可证和完整性；
7. 发布前自动生成 SBOM，并由人工审核 NOTICE。

## 5. Benchmark 证据边界

以下不能作为 KnightFrame 的 20% 证明：

- code-review-graph 的整库读取 65x/376x；
- Headroom 的通用宣传区间；
- OpenSquilla 使用混合/路由模型对比不同 agent 的价格；
- Reasonix、Codex 或 Claude 文档中的产品宣传；
- 不同主模型、不同任务、不同成功率的费用；
- 本地 tokenizer 估算冒充账单。

唯一通过口径见 [08-delivery-verification.md](08-delivery-verification.md)：同任务、同主模型、同质量、冻结配置、供应商真实 usage/账单。
