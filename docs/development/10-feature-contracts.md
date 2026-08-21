# 10 — 功能存在合同

## 1. 规则

下表是功能注册表的人工可读版本。每一行都必须在实现时对应 manifest、事件生产者、UI 消费者、指标记录点和 Windows acceptance test。

状态开关只能说明“允许执行”，不能证明本轮执行。只有产生对应 receipt 的运行才计入功能使用。

## 2. 核心运行时

| Feature ID | 默认 | Trigger | Receipt | Metric | Windows acceptance |
|---|---|---|---|---|---|
| `task.manager` | On | 接受一个用户 Turn | Task 创建/步骤/终止小票 | 完成、取消、阻塞、恢复、schema token | 崩溃恢复、steer、取消、验证证据 |
| `clarification.large_input` | On | 当前文本 grapheme `>200` | 弹窗选择与 Task `Clarifying` | 触发/接受率、轮数、返工、额外 token | 中英 200/201、emoji、取消不丢文本 |
| `style.caveman_lite` | Lite | 每个主模型会话 | 设置状态与输出长度趋势 | 输出 token、追问/展开率 | 中英短规则、Off 后 prompt 消失 |
| `usage.ledger` | On | 每个模型 response/工具结束 | 独立计量小票 | usage 完整率、账单误差 | 崩溃结算、币种、缺失字段不伪零 |
| `event.ledger` | On | 每个持久事件 | UI 可重放状态 | 写入失败、恢复时间、丢事件 | 异常退出后重放到一致状态 |

## 3. 主/副模型

| Feature ID | 默认 | Trigger | Receipt | Metric | Windows acceptance |
|---|---|---|---|---|---|
| `model.primary` | Required | Task 进入执行 | model/provider、流、usage、终止 | TTFT、E2E、质量、费用 | 各 adapter fixture + canary |
| `aux.requirement_reducer` | Off | 用户开启 + 长输入 + 净收益/隐私 gate | executed/skipped、前后 token、净费用 | 净节省、语义保持、延迟、失败 | 短请求零调用；代码/数字/否定保留 |
| `aux.skill_selector` | Off | 用户开启 + 本地 Skill 候选 >1 | 候选变化、费用与延迟 | 选择精度、相对本地增益 | 副模型只见候选摘要；失败回退本地 |
| `aux.memory_judge` | Off | 记忆开启 + 用户开启 + 每 Turn 一次 | needed、检索词、最终注入数 | 判断精度、费用、延迟 | 只见当前请求；不见/不写记忆库 |
| `aux.context_compactor` | Off | 用户开启 + 确定性压缩仍超预算 | before/after、保留锚点、费用 | 连续性、重读、净节省 | 目标/约束/文件/失败/下一步全部保留 |
| `model.capability_probe` | On configure | 新 endpoint/model/profile 变化 | 能力矩阵与退化原因 | probe 成功/误判/变化 | 开源 endpoint fixture + 用户覆盖 |

## 4. 项目速查

| Feature ID | 默认 | Trigger | Receipt | Metric | Windows acceptance |
|---|---|---|---|---|---|
| `project.full_index` | On | 首次打开/不兼容 generation | 阶段、进度、覆盖、失败、Ready | 首建 p95、files/s、覆盖、库大小 | 所有未忽略文件有记录；失败可定位 |
| `project.incremental` | On | edit/外部保存/Git/配置变化 | Ready/Updating/Stale 与切代 | edit-to-fresh、漏失效、误重建 | rename/move/delete/checkout/大小写/junction |
| `project.search` | On | `project.find/show` | 结果、总数、歧义、新鲜度 | top-1、MRR、p95、默认 token | 中英、短词、无结果原因 |
| `project.relations` | On per language | `project.links` | 关系、证据位置、置信度 | precision/recall、歧义 | golden corpus 的 caller/callee/ref/test |
| `project.impact` | On when verified | edit 前/显式 impact 查询 | 静态预测、方向、测试 | 独立真值 precision/recall | 不用同图自证；不称真实执行流 |
| `project.visual` | On | 用户展开项目检查器 | 局部子图/列表 | 加载与交互性能 | 大项目不渲染全图毛线团 |

## 5. 工具与压缩

| Feature ID | 默认 | Trigger | Receipt | Metric | Windows acceptance |
|---|---|---|---|---|---|
| `tool.project` | On | 符号/关系/路径侦察 | 最小位置与 next | 调用/token、避免 broad read | 常见任务不先 ls/全库 grep |
| `tool.read` | On | 精确符号/范围/artifact | path/id、版本、range、total | 读取字节、重复复用、token | 中文/长路径/大文件/分页 |
| `tool.edit` | On | 主模型请求精确 change | vN→vN+1、字符统计、最小 diff | payload/变更比、冲突、回滚 | Unicode 单字符、多文件原子、stale |
| `tool.run` | On | 测试/构建/任意命令 | exit、事实、诊断、raw ID | 耗时、输出压缩、取消 | 隐藏窗口、PowerShell、项目外路径、用户停止 |
| `tool.projection` | On | 工具结果超过短结果门槛 | raw→model 量、完整性、A# | 压缩率、遗漏、重读、耗时 | JSON/test/build/log/diff/dir golden |
| `tool.artifact` | On | 产生原始大结果/完整 diff | 短句柄与存储状态 | 回取、过期、磁盘、命中 | 会话隔离、崩溃完整性、LRU |
| `tool.external` | Only MCP enabled | 用户启用 MCP 且模型查/调用 | 工具短 ID、调用/权限/结果 | schema 节省、调用成功、退化 | 无 MCP 时 schema 完全消失 |
| `policy.full_access` | On | 会话执行 | 完全访问状态、引导/停止回执 | 取消延迟、遗留进程、交易拦截 | 项目外读写/命令成功；用户停止生效；真实交易永久拒绝 |

## 6. 缓存与上下文

| Feature ID | 默认 | Trigger | Receipt | Metric | Windows acceptance |
|---|---|---|---|---|---|
| `context.stable_prefix` | On | 每个主模型调用 | 本地 cache receipt | read/write/fresh、break reason | 确定性序列化、动态尾部不破坏 |
| `context.local_reuse` | On | 同版本 read/query/reducer 重复 | reused 来源 | 命中、避免调用、延迟 | 文件/图 generation 变化自动失效 |
| `context.deterministic_trim` | On | 历史工具投影超预算 | before/after、artifact 保留 | 节省、重读、完整性 | 不删除原始会话，不循环 compact |
| `cache.keepalive` | Conditional | 期望净收益为正且无用户任务 | 成本/收益/调度原因 | 实际复用、费用、抢占 | 默认不固定周期调用；主任务立即让路 |

## 7. 插件与 Plugin Studio

| Feature ID | 默认 | Trigger | Receipt | Metric | Windows acceptance |
|---|---|---|---|---|---|
| `plugin.discovery` | On | 配置的插件根变化 | 条目、协议、兼容/拒绝原因 | 扫描耗时、有效率 | 不扫描任意磁盘；无插件时零 prompt |
| `plugin.runtime` | Entry off | 用户启用条目 | Pending/Active/Failed/Disposed、依赖、版本 | 启动/停止、崩溃、孤儿进程 | 多语言 stdio fixture；Job Object 清理 |
| `plugin.cordis_compat` | On import | 读取 `cordis.yml` | 映射字段与退化项 | 字段执行率、配置回滚 | id/inject/intercept/isolate fixture |
| `plugin.ui_slots` | Active plugin only | 合法声明式 UI contribution | slot/component/id | 注册/撤销、渲染失败 | 未知 slot 拒绝；停用后无残留 |
| `studio.canvas` | On | 用户打开插件工作室 | target、dirty、preview 状态 | 编辑延迟、undo 正确率 | 内置 Tauri 子窗口；不依赖 localhost |
| `studio.ask_knightframe` | User action | 用户提交白板需求 | 目标、布局摘要、会话/turn | 传输字节、送达、失败 | 主窗口收到一次；不暗调副模型 |

## 8. Skill 与记忆

| Feature ID | 默认 | Trigger | Receipt | Metric | Windows acceptance |
|---|---|---|---|---|---|
| `skill.local_router` | On | 每 Turn | 入选 Skill、原因、token | precision、采用、误/漏触发 | 中英、catalog 固代、无命中零注入 |
| `skill.compatibility` | On install | 安装/更新 Skill | Supported/Degraded/Unsupported | 字段执行率、退化数 | 接受字段均有行为或明确错误 |
| `skill.opt` | On background | 有足够真实使用数据且主任务空闲 | 优化建议与采纳结果 | 采纳后精度/token 变化 | 不阻塞 Turn、不自动改 Skill |
| `memory.store` | Off | 用户显式开启并 Remember/导入 | 层/scope/来源/操作 | 每层写读删、冲突、撤销 | 默认零库；L0–L8 全链路 |
| `memory.local_recall` | Memory on | 本地请求匹配过门槛 | 条目/层/原因/token | precision、实际引用、串 scope | global/workspace/session 隔离 |
| `memory.promotion` | Memory on, confirm | 重复/冲突形成候选 | 候选、确认、历史 | 合并质量、撤销、深度分布 | 不自动 LLM 合成；可回滚 |

## 9. UI 与发布

| Feature ID | 默认 | Trigger | Receipt | Metric | Windows acceptance |
|---|---|---|---|---|---|
| `ui.i18n` | On | locale 初始化/切换 | 当前 locale、fallback 诊断 | 缺失/泄漏/hardcode | key/placeholder parity；中英截图 |
| `ui.theme.knight` | Default | 应用启动 | 主题设置 | GPU/动画/可访问性 | 黑白灰、无偏蓝、reduced motion |
| `ui.tool_cards` | On | 工具事件 | 状态/结果/诊断三层 | 布局/展开/大输出性能 | 无遮挡、日志虚拟化、错误可读 |
| `ui.usage_panel` | On | usage 更新 | 时间/token/cache/费用分栏 | 刷新延迟、数值异常 | 窄/宽/DPI 不覆盖对话和进度 |
| `release.integrity` | On | 打包/下载/安装 | 签名/校验/版本 | 失败率、可复现性 | CI、下载、干净 VM 三段验证 |
| `privacy.telemetry` | Off | 未来仅用户逐次同意 | 上传预览和结果 | 上传量、拒绝、删除 | 默认网络中无遥测/硬件 ID |

## 10. CI 对表规则

对每个 `verified` 或 `shipped` Feature ID，CI 需要找到：

```text
manifest entry
trigger handler test
receipt event producer test
metric recorder test
UI/resource coverage test
Windows acceptance test ID
```

任一缺失时状态自动降回 `implemented`，正式 UI 不得显示为可用。实验功能可以保留数据，但必须带清晰 `experimental` 标识，不能参与正式宣传和 20% 成本结论。
