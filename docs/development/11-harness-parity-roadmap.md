# 11 — Harness 功能对齐与精简路线图

> 证据冻结：2026-08-13。本文描述的是对齐目标与当前缺口，不把“已有类型/界面”计作功能完成。

> **2026-08-13 执行策略更新：** KnightFrame 固定为不限时完全访问，不设工具轮数上限，允许项目外路径和任意命令；运行中可追加引导或由用户停止。真实交易、下单、持仓变更与自动购买永久禁止。下表中的“8 轮”、固定超时、工作区边界和逐次审批均已被本决策取代；Windows Job Object 仍负责停止整棵进程树。

## 1. 总原则

**功能对齐与精简同等重要。** KnightFrame 的目标是用更短、更稳定、更精确的收发完成完整的 agent 工作流，而不是通过删除能力制造 token 优势。

必须同时成立：

1. **最小收发不等于功能删减。** 可压缩 schema、投影、重复历史、日志和展示文本；不可删掉工具能力、审批、安全边界、恢复、撤销、多模态或供应商差异。
2. **本地完整、模型最小。** 完整会话、项目图谱、工具原始结果、权限回执和用量账本留在本地；主模型只收到完成当前决策所需的最小投影。
3. **能力显式协商。** provider、模型、工具、MCP、浏览器与输入类型先探测能力；不支持时返回 `Unsupported/Degraded`，不能静默丢字段或伪装成功。
4. **稳定前缀优先。** 系统合同、开启的功能片段与工具 schema 必须确定性排序；动态状态放在尾部。内部长 ID 不跨模型边界，模型看到 `T4/C7/A3` 等短 ID。
5. **每次精简都可追溯。** 投影必须带 `completeness/total/omitted/next/rawArtifactId`；压缩必须带来源范围和失效原因；用户可取回原文。
6. **默认不用额外模型。** 需求压缩、Skill/记忆副模型均按用户开关启用；本地关键词、全量项目速查和确定性投影是默认路径。
7. **节省必须按成功任务结算。** 相同主模型、参数、初始项目、任务与成功标准下，KnightFrame 平均实际费用不高于 Claude Code 的 80%，且完成率和严重错误率不劣化。

## 2. 证据与许可边界

| 来源 | 可用证据 | 本文使用方式 |
|---|---|---|
| Codex | 本地 Apache-2.0 源码、公开 app-server 协议、官方文档 | 对齐事件、会话、权限、MCP、Skill、子代理等公开合同；不复制内部 prompt |
| OpenCode | 本地 MIT 源码、官方文档 | 对齐 runner、工具、投影、权限、压缩、provider 与 CLI 的公开行为 |
| Claude Code | Anthropic 官方文档与公开 CLI 行为 | 只作为产品行为基线；不复制实现、prompt、常量或私有协议 |
| KnightFrame | 当前 Rust/Tauri/Svelte 工程 | 逐文件确认 `current`，只有存在可执行链路与测试证据才算实现 |

本地 `D:\Game\Minecraft\Eps+\Epsilon-26.1.x\claude-code-2.7.1\claude-code-2.7.1\package.json` 明确把自身描述为 “Reverse-engineered Anthropic Claude Code CLI”。该目录因此**不属于实现证据**：不得复制、改写或据其推断私有算法；若它提示某个行为，必须回到 Anthropic 官方文档验证后才能进入验收合同。

## 3. 阶段门

| Phase | 目标 | 退出条件 |
|---|---|---|
| P1 — 可依赖核心 | 完整 agent loop、类型化流、核心工具、安全审批、持久会话、provider 能力、正确用量、双语 GUI | 崩溃恢复后可继续；危险操作无绕过；标准工具链闭环；事件与账本可重放 |
| P2 — 上下文效率 | 压缩、缓存编译、回滚、文件输入、项目速查与投影达到可度量节省 | 长会话不失约束；热缓存命中可解释；字符级编辑与原始结果回取通过 |
| P3 — 扩展与并行 | Hooks、Skills、MCP、子代理/工作树、agent 浏览器、图片输入、headless CLI | 可选能力关闭时零 prompt/零 schema/零进程；开启时具备审批、隔离和完整回执 |
| P4 — 发布证明 | 本地可观测、选择性遥测、跨 provider/模型矩阵、GUI/CLI 一致性、成本质量基准 | 同口径成功任务成本至少低 20%；质量不劣化；Windows 安装包全部门禁通过 |

P1 是首个可用版本门槛；P2/P3 不能用“省 token”作为省略公开核心能力的理由。阶段可以并行开发，但不得越过前置安全门发布。

## 4. 功能对齐矩阵

### 4.1 核心运行时

| 层 | Codex 公开行为 | Claude Code 公开行为 | OpenCode 公开行为 | KnightFrame current / Phase | 节省机制 | 不允许牺牲的功能 | 验收 |
|---|---|---|---|---|---|---|---|
| **H01 Agent loop** | `thread → turn → item/tool → completed`；可 stop、steer、resume、fork；协作调用是类型化 item。[C1] | 交互与 headless 都能多轮调用工具；有 `--max-turns`、权限模式、继续/恢复。[A1] | session runner/coordinator 驱动模型—工具循环；primary/subagent 和内部 compaction/title/summary agent 分层。[O1][O4] | 已有最多 8 轮的模型—工具循环、取消和 Task 更新；canonical `HistoryItem` 跨用户 turn 保留 user/assistant/tool call/tool result，并在下一 turn 投影回 provider；错误策略仍粗粒度，历史尚未持久化。`agent_loop.rs`、`session.rs`、`types.rs`。**P1** | 短稳定系统合同；只投影启用工具；优先 `project` 定位后精确读写 | 任意合法多轮工具链、取消传播、终态原因、assistant/tool 历史、多个独立会话 | 模拟 `tool→tool→text`、工具失败、达到轮限、用户取消；事件不丢不重，取消后进程树终止，第二 turn 能引用第一 turn 事实 |
| **H02 Stream** | `turn/started → item/started → delta → item/completed → turn/completed`；usage 独立通知，完成事件有最终状态。[C1] | print 支持 text/json/stream-json，交互模式持续展示工具和结果。[A1][A10] | 公共事件 manifest 与 session event/projector 提供结构化增量。[O1] | SSE 能发 reasoning/text/usage、tool started/completed、assistant completed/failed/cancelled；工具失败与取消会写入配对 `ToolResult`，失败通过 `tool.completed` 中的 `projection.status=failed` 表达；仍无持久事件序号、重连/重放和独立 `tool.failed` 事件。`provider.rs`、`agent_loop.rs`、`session.rs`。**P1** | delta 只传新增内容；快照与 delta 不重复；UI 不从日志反推状态 | 有序生命周期、终态、usage、工具状态、断线恢复；只展示 provider 可公开 reasoning summary，不泄露隐藏 CoT | 对每个 `eventId` 做幂等重放；随机断流后从游标恢复；所有 started 恰有 completed/failed；UI 正文、工具、诊断分层一致 |
| **H03 Tool call protocol** | 原生、动态与 MCP 工具共享稳定 call/item ID；工具调用和输出为独立 item。[C1] | 工具可通过 allowed/disallowed、permission prompt 与 schema 约束；headless 输出保留调用结构。[A1] | 内置/自定义/MCP 工具统一 registry；每工具独立 schema 与权限。[O2][O3] | 能按 index 累积流式 tool arguments，限制 16 calls/64 KiB；5 个内置函数与本地 artifact 投影；成功、失败和取消都形成 call/result 配对，失败含 `errorKey/status/completeness`；仍无 schema 版本协商与批准阶段。`provider.rs`、`agent_loop.rs`。**P1** | 短工具名、最小 schema、稳定排序；大结果只回投影和短 artifact ID | 并行/串行调用语义、参数校验、调用 ID、完整原始结果、失败类型、取消、审批 | 分片参数、乱序 index、未知工具、超限、无效 JSON、失败/取消 fixtures；模型与 UI 得到同一 call 状态，原始结果可按 artifact 取回 |
| **H04 Context + compaction** | 有显式 compact；resume 可排除 turn 并分页读，避免重建全历史；压缩有标准通知与测试。[C1][C2] | 支持自动上下文管理与 `/compact`，会话继续/恢复保留工作连续性。[A1][A11] | durable messages 不改写；活跃上下文由结构化 checkpoint + recent tail 替换，溢出只重试一次，旧工具输出可裁剪。[O5] | 已有内存 canonical history，下一 turn 会带回 user/assistant/tool call/tool result；尚无持久化、上下文预算编译器、压缩或 summary，长会话仍会把全部内存历史重新发送。`agent_loop.rs`、`state.rs`。**P2** | 稳定规则前缀 + unresolved state + recent tail；项目事实按查询取，不倾倒目录；旧工具原文留本地 | 用户约束、未完成任务、审批、文件版本、工具错误、引用来源、最近对话；压缩前后语义连续 | 超窗口长会话能继续同一任务；golden checkpoint 包含所有未决项；压缩前后任务完成率不降；手动/自动压缩有 receipt 和可审计来源范围 |
| **H05 Cache** | provider usage 可报告 cached input；Codex 会话/配置围绕稳定上下文与 token usage 结算。[C1] | 官方公开 prompt caching 与成本/usage 指引。[A11][A12] | provider usage 归一化 cache read；compaction 给 recent tail 留固定空间。[O5] | 只解析/展示 `cached_tokens`，没有稳定前缀编译、cache write、break reason 或跨 turn 复用策略。`provider.rs`、`types.rs`。**P2** | append-only 稳定前缀；确定性功能片段和 schema；动态信息尾置；关闭能力完全不入 prompt；长哈希不发给模型 | provider/账号/模型/项目隔离、正确失效、隐私边界、冷/热态可区分 | 同会话只追加用户 turn 时公共前缀字节稳定；变更模型/权限/Skill 后明确失效；账单 token 与本地 fresh/cache read/write 对账；按 provider 输出 break reason |
| **H06 Tools** | shell、patch、MCP/resource 等形成完整编码工具面，Windows sandbox 独立实现。[C1][C3] | 公开工具、MCP、浏览器与权限规则支持读、搜、编辑、运行和外部能力。[A2][A3][A8] | read/edit/apply-patch/bash/grep/glob/LSP/skill/question/web 等内置工具；大输出存 artifact，模型只收限量投影。[O2][O6] | 已有 `project/read/edit/run/task`：范围读最多 800 行、唯一片段原子替换、静默进程、root 越界检查；project 目前只按文件名查询 manifest，不含符号/引用；run 原始输出只在内存且最多 128 KiB。`tools.rs`、`project.rs`。**P1 核心，P2 全量速查** | graph-first；字符级 exact edit；按行/符号 read；原生结构化结果；内容专用投影；原文不进 prompt | 目录元数据不能只剩名称；搜索、符号、引用、读、增删改、补丁、命令、测试、诊断、原文回取与 Windows 路径语义 | 工具 parity 清单逐项 fixture；单字符 edit、CRLF/UTF-8/中文路径、冲突/多匹配不落盘；目录含类型/大小/时间；图谱 top-1/引用召回；截断必有 `next/rawArtifactId` |
| **H07 Approvals + sandbox** | approval policy 与 sandbox profile 分离；公开协议含 permission profiles、网络/计算机使用限制，Windows 有专用 sandbox；审批可由 auto-review 子代理辅助但不能绕过策略。[C1][C4] | permission modes、allow/ask/deny 与 sandboxed Bash；读操作和状态改变可区分批准。[A1][A4] | 有规则优先级与 allow/ask/deny；缺失权限默认拒绝，决策可保存。[O3] | 只有路径 root、URL scheme 和进程取消约束；`run/edit/browser` 没有用户审批、命令策略、网络隔离、Windows job/ACL/WFP sandbox。**发布阻断，P1** | 本地规则先裁决，只有 `ask` 才打断用户；保存窄范围批准，不把整套策略发给模型 | 默认最小权限、拒绝优先、命令/文件/网络/browser/MCP 分域、子进程继承、收据、撤销；不能用 token 优化绕过确认 | 默认 profile 的危险写/外网/提权/项目外路径均 ask/deny；批准范围不可扩大；应用崩溃不遗留子进程；每次决策有 rule/source/scope/expiry receipt |

### 4.2 扩展、并行与多模态

| 层 | Codex 公开行为 | Claude Code 公开行为 | OpenCode 公开行为 | KnightFrame current / Phase | 节省机制 | 不允许牺牲的功能 | 验收 |
|---|---|---|---|---|---|---|---|
| **H08 Hooks** | hooks 可按 cwd 列出；linked worktree 从 root checkout 解析；禁用 hook 仍可见；非托管 hook 需 hash/trust 才能运行。[C1] | 官方定义多个生命周期事件、matcher、command/prompt/agent hook、async 与退出码合同。[A5] | plugin host 暴露 hook 扩展点。[O7] | 无 hook registry、事件或信任模型。**P3** | 未配置时零扫描/零 prompt；matcher 本地执行；事件 payload 只给必要字段 | 生命周期覆盖、超时、取消、失败策略、信任/来源、工作树解析、禁止静默修改主任务 | 每事件 contract test；未知/未信任 hook 不执行；超时不挂死主 loop；hook 输出明确进入 context、block 或仅日志；关闭后热路径无 hook 字节 |
| **H09 Skills** | skills 可列举/加载，支持插件提供；公开 Skill 合同与 app-server 列表接口。[C1][C5] | 官方 Agent Skills；按描述发现，内容按需加载；subagent 可限制 skills。[A6][A7] | 本地 discovery/guidance 与 `skill` 工具；agent 可配置技能。[O4][O8] | `cavemanMode=lite/off` 已真实控制一条稳定短输出规则；尚无通用 Skill 发现、关键词注入或 SkillOpt。**P3** | 默认本地关键词路由；只注入选中 Skill 摘要，正文按需读；副模型路由默认关；SkillOpt 离线/低频 | 来源优先级、显式/自动触发、依赖资源、版本、禁用、冲突、实际执行回执；Caveman Lite 真正生效 | 中英文关键词、显式点名、歧义、禁用、资源缺失 fixtures；未命中时零 Skill token；命中时 receipt 列 skill/version/原因/注入字节；Lite 输出长度回归 |
| **H10 Subagents + worktree** | thread 可 fork；协作 item 支持 spawn/send/resume/wait/close；thread 带 parent、agent role；支持 git worktree 环境。[C1][C6] | custom subagents 有独立 context、工具/权限/skills；可前台/后台运行并支持隔离工作树工作流。[A7][A13] | primary/subagent 分层；项目层有 git worktree create/list/remove 策略。[O4][O9] | 无子代理、fork、worktree 或父子会话图。**P3** | 子代理只收目标、最小项目切片和允许工具；结果回传短 artifact/摘要；不复制主会话全历史 | 父子关系、独立权限/预算/取消、并发上限、工作树所有权、冲突处理、恢复、完整产物 | 2 个并行 agent 不写同一工作树；父取消向下传播；崩溃后可认领/清理；结果包含 commit/diff/test/artifact；上下文输入显著小于复制主历史 |
| **H11 MCP** | app-server 支持 OAuth、reload、server status、tools/resources/templates 分页、resource read/tool call。[C1][C3] | 支持 stdio/HTTP 等 MCP、scope、OAuth/认证、资源和项目配置批准。[A3] | local/remote MCP、OAuth、启停、global/per-agent 管理；官方提醒大量 schema 会迅速占满 context。[O10] | 无 MCP client、registry、OAuth、resource 或审批。**P3** | 默认不常驻全部 schema；server 摘要 → 本地搜索/选择 → 延迟加载工具 schema；结果统一 artifact 投影 | transport、OAuth、resources/prompts/tools、超时、取消、server health、项目配置信任、权限和完整错误 | 官方 conformance server；本地/远程/OAuth/断线/超时/分页 fixtures；1000 工具时首轮只投影选中 schema；关闭 server 后零 schema、零进程、零网络 |
| **H12 Browser** | ChatGPT/Codex 桌面产品公开共享 browser 能力，但官方明确 Codex CLI/IDE 无内置 browser；安全合同把 computer use 与网络权限纳入 profile。[C4][C10] | Chrome 集成能读 DOM/console/network、截图、交互、上传和录制；只读与状态改变采取不同审批，site permission 独立。[A8] | Web/search/fetch 属于工具面；交互浏览器不是所有运行模式的强制内核能力。[O2] | 有独立 WebView2 窗口，仅 open/navigate/back/forward/refresh/close；不是 agent tool，`canGoBack/loading` 等恒为假，无 DOM/截图、导航事件、站点权限 receipt。`browser.rs`。**P3** | 浏览工具按需启用；优先结构化 DOM/可见文本/定点截图；禁止每轮全页与全部 console；批处理同类只读动作 | 可见真实浏览器、导航状态、DOM/截图/console、文件上传、登录/CAPTCHA 交还用户、站点权限、读写动作审批 | 本地 web fixture 完成读、点击、输入、下载/上传、console 和截图；状态实时更新；未授权站点拒绝；所有状态改变有 receipt；关闭后模型看不到浏览 schema |
| **H13 Image + file inputs** | 公开 Codex 产品支持 image/file inputs；本地协议含 image endpoint/extension 线索。[C7][C11] | CLI 可粘贴图片；Desktop 可附加/拖入 image、PDF 和其他文件；浏览器上传服从 Read 权限和大小限制，具体多模态能力依模型/入口协商。[A8][A10][A13] | session/message 与 provider 转换支持 file/image parts，具体能力由 provider 决定。[O11] | 文件附件按钮明确禁用并说明未提供；后端 `session_send` 只接收 String，无附件类型。**P2 文件，P3 图片** | 附件本地去重；文本只送所需范围；图片按模型上限缩放/切片；元数据和二进制分离 | 文件类型/大小/来源、读取权限、图片方向/透明度、provider 不支持时可见降级、会话恢复可重连附件 | txt/pdf/image fixtures；项目外文件先授权；超限/不支持明确失败；provider 收到正确 MIME/维度；账本记录实际输入 token/bytes，不把 base64 写入会话文本 |

### 4.3 会话、供应商与产品面

| 层 | Codex 公开行为 | Claude Code 公开行为 | OpenCode 公开行为 | KnightFrame current / Phase | 节省机制 | 不允许牺牲的功能 | 验收 |
|---|---|---|---|---|---|---|---|
| **H14 Session resume** | `thread/start/resume/read/fork`；resume 重放 usage，可排除 turn、分页取历史。[C1][C2] | `--continue`、`--resume` 和会话选择器恢复工作。[A1][A10] | durable session 存储与 resume 入口；runner 从既有会话继续。[O1] | session/task/artifact/project 全在内存；只持久化 settings；重启即丢，会话快照也无 active session。`state.rs`、`session.rs`、`settings.rs`。**P1** | 本地 append-only 事件账本；按需分页物化；只重编译恢复点后的最小上下文 | 用户/assistant/tool/approval/usage/task/file-version 完整性，崩溃一致性、跨版本迁移、附件引用 | 在模型流、edit、tool 完成各时点强杀应用并恢复；无重复编辑/扣费/消息；恢复后 active task 和 usage 对账；1000 turns 不全量载入 UI/模型 |
| **H15 Rollback** | `thread/rollback` 删除最后 N turns 并持久化 rollback marker。[C1][C2] | checkpointing 可恢复代码、对话或两者，覆盖每次用户 prompt 前的编辑状态。[A9] | snapshot/revert 支持 restore/stage/clear/commit。[O12] | 无 turn rollback、文件 checkpoint 或 undo ledger；exact edit 原子落盘但不可撤回。**P2** | 保存反向 patch/文件版本和事件指针，不复制整个项目；定期 checkpoint 合并 | 只回对话、只回代码、两者同时；外部改动检测；二进制/未跟踪文件策略；撤销本身可审计 | 多文件 edit + run 后三种回滚模式；用户并发修改时拒绝覆盖并给冲突；重启后仍可回滚；工作树干净度和会话指针正确 |
| **H16 Models + provider** | model/list 与能力元数据；Codex API/provider 层独立于 app-server 协议。[C1] | 模型别名、完整名称与 Bedrock/Vertex/Foundry 等部署入口；功能随认证/provider 有差异。[A12][A14] | 大量 provider adapter、模型目录、provider-specific options 与归一化能力。[O13] | 硬编码 OpenCode Zen + `deepseek-v4-flash-free` + bearer `public`；只探测模型是否在 `/models`，宣称 stream/reasoning/toolCalls；用户不可选其他 provider/model。`provider.rs`。**P1 adapter，P4 matrix** | capability probe 后只发送供应商支持字段；provider 专用适配，不用巨型“兼容 API”；设置只存选择，不把目录注入 prompt | 用户自选、认证安全、stream/tool/reasoning/image/cache/usage 差异、错误归一化、退避/限流，不静默换模型 | Anthropic/OpenAI-compatible/OpenCode/至少一款本地开源模型 contract suite；每能力 Unsupported/Degraded/Supported；不支持工具时发送前阻断；密钥不进日志/事件 |
| **H17 Usage** | turn/session usage 生命周期与 account usage 查询；resume 可重放 token usage。[C1] | `/cost`、状态行、usage monitoring 和 OpenTelemetry 指标覆盖 token/费用/活动。[A11][A15] | provider usage 归一化 input/output/cache read 等，session 可聚合。[O1][O13] | 解析 input/cache-read/output/reasoning；每个模型 round 增加 `request_count`，round usage 先聚合为 turn usage，再跨成功 turn 累加到 session；仍无失败/取消 usage 结算、cache-write、时长、速度、费率/真实账单。`provider.rs`、`agent_loop.rs`、`session.rs`、`types.rs`。**P1** | 计量旁路，不回灌 prompt；按事件增量结算；费率目录本地版本化 | turn/session/project/provider 维度，fresh/cache read/write/reasoning/output、请求、时长、速度、费用、来源和缺失值 | 多轮/多工具/失败/取消/恢复聚合不重算；与 provider usage/账单逐 turn 对账；缺字段显示 N/A 不猜；UI 独立面板不与正文混排 |
| **H18 Telemetry** | 本地协议和实现有 telemetry 模块；安全/管理文档覆盖治理。[C4] | 官方 OpenTelemetry 可配置 metrics/events、headers 和隐私设置。[A15] | OTLP 可选配置与 observability 模块。[O14] | 无远程 telemetry；也无持久本地诊断/性能事件账本。**P4（本地账本 P1）** | 本地聚合优先；远程默认关闭；采样、字段 allowlist、脱敏；指标永不进 prompt | 运行诊断、性能、成本、错误率、缓存命中、工具效果；用户知情/关闭/清除；不得收集 prompt、代码、密钥 | 默认网络抓包零 telemetry；开启后 schema allowlist；敏感 fixture 不外发；关闭/清除立即生效；本地 benchmark 能从事件账本复算指标 |
| **H19 CLI + GUI** | TUI、non-interactive exec、IDE/app-server 共用核心协议，CLI 可 JSON 输出。[C1][C9] | interactive、print/headless、text/json/stream-json、pipe、resume 和工具策略 flags。[A1][A10] | TUI、CLI、web/desktop/server 共享 core/session。[O15] | Svelte 5 + Tauri GUI 与 typed bridge 已有 workspace/settings/browser/usage/task；mini 与附件均明确 unavailable；无 headless CLI。**P1 GUI，P3 CLI** | CLI/GUI 消费同一事件；UI 虚拟化；日志折叠；不可用能力隐藏而非把假 schema/假数据加载进来 | 交互、headless、机器可读 stream、stop/resume、审批通道、真实状态、无终端闪窗、键盘/无障碍 | 同一录制事件在 CLI/GUI 得到等价终态；JSON schema golden；CLI exit code；1280×800/1024×768/DPI；不可用功能不可点击；Windows 后台进程无可见终端 |
| **H20 i18n** | CLI/app-server 将结构化错误与呈现分离，可由客户端决定文案。[C1] | CLI 主要为英文公开合同；结构化 headless 输出使客户端可本地化而不改语义。[A1] | config/docs/UI 有多语言入口，core 事件独立于展示。[O15] | `en-US/zh-CN` catalog、localized error key/args、Intl number/time/money 已有；后端仍有 provider/model/display 字符串，需检查所有用户可见硬编码和 key parity。`src/lib/i18n.ts`、`src-tauri/src/error.rs`。**P1** | 事件只传稳定 key + args；catalog 不进模型 prompt；按需加载 locale | 中英文功能等价、错误参数、复数/数字/时间/费用、路径与 CJK 布局、fallback 可见且可测 | catalog key 100% parity；硬编码 lint；伪本地化与长文案截图；中英文 E2E 同样完成创建—工具—审批—恢复—错误流程 |

## 5. 横向验收门

### 5.1 功能非劣化门

每个 Phase 的 parity manifest 必须记录：

```text
CapabilityId
Status = Unsupported | Designed | Implemented | Verified | Shipped
EntryPoints = GUI | CLI | AgentTool | MCP
ProviderConstraints
PermissionDomain
Events
Persistence
Rollback
Tests
```

`Unsupported` 可以在能力探测后显示，但不能：

- 把按钮或 schema 留在热路径里假装可用；
- 丢弃输入后继续运行；
- 将 provider 错误改写成成功；
- 用“更精简”解释缺少恢复、审批、原始结果或回滚。

### 5.2 token 与缓存门

每个 benchmark run 同时保存：

- 实际发送的 system/user/tool schema/tool result token；
- fresh input、cache read、cache write、reasoning、output；
- 每个投影的原始字节、模型字节、完整性与回取次数；
- 每次 cache break 的首个差异片段和原因；
- 成功、失败、取消分别结算，失败不能从平均值中消失。

成本结论只比较成功且质量达标的任务。冷启动、热缓存、短任务、长任务、工具密集任务分别报告，再给加权总结果。

### 5.3 一次运行质量门

沿用既定评分：

```text
完成度 50% + token 效率 30% + 速度 10% + 灵活度 10%
```

每个 agent 对每个不同任务只运行一次；通过扩大冻结任务集控制偶然性。KnightFrame 独立 benchmark 工具不得读取旧会话、旧记忆或未声明缓存来污染结果。

### 5.4 发布阻断项

以下任一未通过，不能标记 `shipped`：

- P1 审批/sandbox、持久会话、usage 对账；
- tool started 没有对应 completed/failed；
- 截断后无法回取原始 artifact；
- GUI 显示了后端不可用能力；
- 默认配置产生未声明副模型请求或远程 telemetry；
- 相同主模型下虽然少 token，但完成率或严重错误率低于基线；
- Windows 安装包出现终端闪窗、路径越界、孤儿进程或恢复失败。

## 6. 精确本地证据索引

### 6.1 Codex（Apache-2.0）

- 许可：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\codex-main\codex-main\LICENSE`
- [C1] app-server 公共协议：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\codex-main\codex-main\codex-rs\app-server\README.md`，重点为 thread/turn/compact/rollback/model/skills/hooks/MCP/stream/collab/usage。
- [C2] 会话测试：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\codex-main\codex-main\codex-rs\app-server\tests\suite\v2\compaction.rs`、`thread_resume.rs`、`thread_rollback.rs`。
- [C3] MCP 测试：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\codex-main\codex-main\codex-rs\app-server\tests\suite\v2\mcp_tool.rs`、`mcp_resource.rs`、`mcp_server_status.rs`。
- [C4] 权限与 sandbox：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\codex-main\codex-main\codex-rs\protocol\src\request_permissions.rs`、`approvals.rs`、`permissions.rs`，以及 `codex-rs\windows-sandbox-rs\src\`。
- [C5] Skill/Hook 测试：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\codex-main\codex-main\codex-rs\app-server\tests\suite\v2\skills_list.rs`、`hooks_list.rs`、`executor_skills.rs`。
- [C6] worktree/子代理公开协议仍以 [C1] 的 `parentThreadId`、`agentNickname/role`、collab item 和官方 worktree 文档为准。
- [C7] image/API 能力线索：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\codex-main\codex-main\codex-rs\codex-api\src\endpoint\images.rs`、`codex-rs\app-server\tests\suite\v2\imagegen_extension.rs`。
- [C8] provider/model：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\codex-main\codex-main\codex-rs\protocol\src\models.rs`、`codex-rs\codex-api\src\endpoint\models.rs`。
- [C9] non-interactive CLI：官方 [Non-interactive mode](https://learn.chatgpt.com/codex/non-interactive-mode)。
- [C10] browser 产品边界：官方 [Browser](https://learn.chatgpt.com/codex/browser)，明确 Codex CLI/IDE 不提供内置 browser，桌面产品提供共享浏览器。
- [C11] 图片输入：官方 [Image inputs](https://learn.chatgpt.com/codex/image-inputs)。

### 6.2 OpenCode（MIT）

- 许可：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\opencode-1.18.1\LICENSE`
- [O1] runner/session：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\opencode-1.18.1\packages\core\src\session\runner\llm.ts`、`runner\index.ts`、`run-coordinator.ts`、`session.ts`、`event.ts`、`projector.ts`。
- [O2] 工具：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\opencode-1.18.1\packages\core\src\tool\read.ts`、`edit.ts`、`apply-patch.ts`、`bash.ts`、`grep.ts`、`glob.ts`、`skill.ts`、`webfetch.ts`、`websearch.ts`。
- [O3] 权限：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\opencode-1.18.1\packages\core\src\permission.ts`、`permission\saved.ts`、`permission\sql.ts`。
- [O4] agents：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\opencode-1.18.1\packages\core\src\plugin\agent.ts`。
- [O5] compaction：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\opencode-1.18.1\packages\core\src\session\compaction.ts`、`config\compaction.ts`。
- [O6] 原始工具 artifact：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\opencode-1.18.1\packages\core\src\tool-output-store.ts`。
- [O7] hook host：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\opencode-1.18.1\packages\core\src\plugin\host.ts`。
- [O8] skills：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\opencode-1.18.1\packages\core\src\skill.ts`、`skill\discovery.ts`、`skill\guidance.ts`。
- [O9] worktree：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\opencode-1.18.1\packages\core\src\project\copy-strategies.ts`。
- [O10] MCP：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\opencode-1.18.1\packages\core\src\config\mcp.ts`。
- [O11] image/file provider 转换：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\opencode-1.18.1\packages\core\src\github-copilot\chat\convert-to-openai-compatible-chat-messages.ts`。
- [O12] rollback：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\opencode-1.18.1\packages\core\src\snapshot.ts`、`session\revert.ts`。
- [O13] providers：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\opencode-1.18.1\packages\core\src\provider.ts`、`config\provider.ts`、`plugin\provider\`。
- [O14] telemetry：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\opencode-1.18.1\packages\core\src\observability\otlp.ts`。
- [O15] 产品入口：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\opencode-1.18.1\packages\cli\`、`packages\desktop\`、`packages\core\src\public-event-manifest.ts`。

### 6.3 KnightFrame current

- agent loop/stream/provider：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\knightframe-rs\src-tauri\src\agent_loop.rs`、`provider.rs`、`session.rs`。
- 工具/项目：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\knightframe-rs\src-tauri\src\tools.rs`、`project.rs`。
- 状态/设置/合同：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\knightframe-rs\src-tauri\src\state.rs`、`types.rs`、`settings.rs`、`lib.rs`。
- 浏览器：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\knightframe-rs\src-tauri\src\browser.rs`。
- 前端 bridge/i18n：`D:\Game\Minecraft\Eps+\Epsilon-26.1.x\knightframe-rs\src\lib\bridge.ts`、`types.ts`、`state.ts`、`i18n.ts`。

## 7. 官方公开链接

### Codex

- [Developer commands / CLI](https://developers.openai.com/codex/cli/reference)
- [Security and approvals](https://developers.openai.com/codex/security)
- [Skills](https://developers.openai.com/codex/skills)
- [MCP](https://developers.openai.com/codex/mcp)
- [Open-source Codex repository](https://github.com/openai/codex)
- [Subagents](https://learn.chatgpt.com/codex/agent-configuration/subagents)
- [Git worktrees](https://learn.chatgpt.com/codex/environments/git-worktrees)
- [Non-interactive mode](https://learn.chatgpt.com/codex/non-interactive-mode)
- [Browser](https://learn.chatgpt.com/codex/browser)
- [Image inputs](https://learn.chatgpt.com/codex/image-inputs)

### Claude Code

- [A1 — CLI reference](https://code.claude.com/docs/en/cli-reference)
- [A2/A3 — MCP](https://code.claude.com/docs/en/mcp)
- [A4 — Sandboxed Bash](https://code.claude.com/docs/en/sandboxing)
- [A5 — Hooks](https://code.claude.com/docs/en/hooks)
- [A6 — Skills](https://code.claude.com/docs/en/skills)
- [A7 — Subagents](https://code.claude.com/docs/en/sub-agents)
- [A8 — Chrome/browser integration](https://code.claude.com/docs/en/chrome)
- [A9 — Checkpointing](https://code.claude.com/docs/en/checkpointing)
- [A10 — Interactive/headless operation](https://code.claude.com/docs/en/interactive-mode) and [programmatic mode](https://code.claude.com/docs/en/headless)
- [Claude Code Desktop and file attachments](https://code.claude.com/docs/en/desktop)
- [A11 — Cost management](https://code.claude.com/docs/en/costs)
- [A12 — Model configuration](https://code.claude.com/docs/en/model-config)
- [A13 — Common workflows](https://code.claude.com/docs/en/common-workflows)
- [A14 — Third-party deployment](https://code.claude.com/docs/en/third-party-integrations)
- [A15 — Usage monitoring / OpenTelemetry](https://code.claude.com/docs/en/monitoring-usage)

### OpenCode

- [O2 — Tools](https://opencode.ai/docs/tools/)
- [O3 — Permissions](https://opencode.ai/docs/permissions/)
- [O4 — Agents](https://opencode.ai/docs/agents/)
- [O5 — Compaction](https://opencode.ai/v2/docs/compaction)
- [O10 — MCP servers](https://opencode.ai/docs/mcp-servers/)
- [O13 — Providers](https://opencode.ai/docs/providers/)
- [O15 — CLI](https://opencode.ai/docs/cli/)

## 8. 决策结论

KnightFrame 应追求的是**完整能力面上的最小充分传输**：

- 功能面至少覆盖公开基线；
- 控制面只加载当前启用能力；
- 项目全量理解留在本地速查库；
- 工具原文留本地，模型只收有回取路径的最小投影；
- 安全、恢复、回滚、用量和扩展能力不能因节省 token 被降级；
- 任何“比 Claude Code 省 20%”的结论，必须在上述功能和质量门通过之后才成立。
