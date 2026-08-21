# 08 — 分期交付、验证与 20% 成本门禁

## 1. 交付原则

KnightFrame 不按“功能数量”推进，而按可验证的垂直能力推进。每个阶段必须同时交付：

- 运行代码；
- UI 回执；
- 中英文资源；
- 指标；
- Windows 打包验证；
- 成本与质量影响；
- 文档和功能注册表状态。

任何阶段不得以 mock UI、设置开关或单元测试替代真实端到端行为。

## 2. Phase 0 — 文档与基线冻结

### 目标

在写核心代码前冻结产品合同、协议边界和 benchmark。

### 交付

- 本目录全部架构文档；
- 功能注册表 schema；
- 主/副模型 ExecutionTarget schema；
- 核心工具短 schema；
- provider capability 与 usage 规范；
- 参考项目许可证/来源清单；
- Claude Code 对比版本、配置和模型；
- 固定任务集、质量判定和账单采集方案；
- Windows 测试矩阵。

### 验收

- 需求追踪矩阵无未落地项；
- 所有“默认开启/关闭”一致；
- 没有把最终回答压缩写成副模型职责；
- 项目速查明确本地全量、模型结果精简；
- 所有外部 benchmark 声明为参考而非本项目成绩。

## 3. Phase 1 — Rust 骨架、事件与黑白桌面壳

### 目标

建立可打包、可恢复、无终端窗口的最小产品骨架。

### 交付

- Cargo workspace 和 crate 边界；
- Tauri 2 + Svelte 5；
- 类型化事件协议、event ledger、IPC 合并；
- Session/Turn/Task 基础状态机；
- TaskManager 默认开启；
- 黑白设计 token、顶栏、对话、输入框、空白态剑徽；
- en-US/zh-CN 统一 catalog 与硬编码扫描；
- Windows 安装、启动、更新、卸载骨架。

### 验收

- 打包应用启动不弹终端；
- 10,000 个流事件 UI 不阻塞；
- 崩溃后恢复最后完整 Turn 状态；
- 200/201 grapheme GrillMe 边界正确；
- 所有组件文本通过 i18n lint；
- 空白态和最小窗口视觉回归无失衡/遮挡。

## 4. Phase 2 — Provider 核心与正常对话

### 目标

先完成可靠的主模型对话，不接工具也能正确流式、计量和显示。

### 交付

- ExecutionTarget 与 profile 管理；
- OpenAI Responses、Anthropic Messages、DeepSeek 原生、OpenRouter；
- OpenAI-compatible capability probe；
- 文本、reasoning summary、usage、错误事件；
- 主/副模型独立客户端和配额；
- 计时/token/cache/费用独立 UI；
- OpenCode 免费模型推荐 profile 的实时可用性检测。

### 验收

- 每 adapter fixture + canary；
- 任意 SSE chunk 边界解析；
- usage 缺失不伪造 0；
- reasoning 不与最终回答混合；
- 所有副模型关闭时零副模型请求、零相关 prompt；
- 主模型切换有缓存断裂回执。

## 5. Phase 3 — 全量项目速查

### 目标

先建机器真正可用的全量项目资料库，再让 agent 获得代码工具。

### 交付

- 文件发现、ignore、SQLite generation；
- tree-sitter 语言适配；
- 全文、符号、关系、配置和测试索引；
- 首次构建门与进度 UI；
- watcher、Git 与 edit 增量更新；
- `project.find/show/links/impact`；
- 项目搜索与局部关系 UI。

### 验收

- 所有未忽略文件有记录；
- Full/Partial/Text/Failed 正确；
- 精确 symbol top-1 目标 ≥99%；
- 已发布语言 reference recall 目标 ≥95%；
- warm query p95 目标 <100ms；
- 默认模型投影目标 ≤300 token；
- 更新采用 generation 原子切换，无混合快照；
- stale 查询不返回旧 edit 位置。

## 6. Phase 4 — 极简工具与精确修改

### 目标

形成可完成真实修复任务的最小 agent loop。

### 交付

- `project/read/edit/run/task`；
- 字符级 edit、短文件版本、多文件事务；
- Windows 命令解析、Job Object、隐藏窗口、取消；
- 工具 RawArtifact/ModelProjection；
- JSON/test/build/log/search/diff/dir reducer；
- 工具卡、edit 卡、诊断日志与 artifact 回取；
- 权限 deny/ask/allow。

### 验收

- 单字符修改只传变化字符；
- stale/old mismatch/重叠 edit 零写入；
- 多文件事务失败零部分提交；
- 工具错误保留 exit code 与中部关键诊断；
- 取消完整进程树；
- 正常任务不先 ls/全库 grep；
- 真实代码任务可端到端完成并验证。

## 7. Phase 5 — 稳定前缀、缓存与成本账本

### 目标

在已有可用 harness 上实现并证明核心省钱机制。

### 交付

- 稳定前缀编译器；
- 确定性 tools/system 序列化；
- append-only 会话视图；
- cache break 归因；
- read/query/reducer 本地复用；
- 确定性历史工具投影压缩；
- provider billed/estimate 分离的 usage ledger；
- 成本、缓存和节省 UI 小票。

### 验收

- 重复稳定前缀可复现 cache read；
- 动态 Task/进度不破坏前缀；
- 每个模型调用（含副模型/compaction）进入账本；
- 冷输入、cache write 和 reasoning 不漏算；
- 真实账单 adapter 聚合误差目标 ≤1%；
- 不发固定周期的无收益 keepalive。

## 8. Phase 6 — Skill、Caveman Lite 与 SkillOpt

### 目标

按需提供能力，不让完整 Skill 目录常驻上下文。

### 交付

- Skill package loader、catalog generation、兼容矩阵；
- 本地关键词/FTS/子串路由；
- 当前 Turn catalog 固定；
- 只注入入选 Skill；
- Caveman Lite 默认稳定短规则；
- Skill 实际执行回执；
- SkillOpt 确定性离线统计与建议审核 UI；
- 可选 SkillSelector 副模型。

### 验收

- 无命中时零 Skill prompt；
- 中英文短查询路由；
- Turn 中途更新不混 catalog generation；
- 所有接受字段有行为或明确退化；
- Caveman Lite 关闭后相关 prompt 完全消失；
- SkillOpt 不占主任务槽、不自动改 Skill；
- 副模型关闭时无调用。

## 9. Phase 7 — 九层记忆（默认关闭）

### 目标

只实现有触发、有回执、可撤销的九层记忆，不复活 16 维和无形自动学习。

### 交付

- 独立可选 memory store；
- L0–L8 层级、global/workspace/session scope；
- 显式 Remember、导入、编辑、删除、导出；
- 本地关键词读取判断；
- 注入回执和使用统计；
- 晋升/合并候选人工确认；
- 可选 MemoryJudge 副模型。

### 验收

- 默认关闭时零数据库、零扫描、零调用、零 prompt；
- 主模型无 memory write；
- 九层各有端到端 fixture；
- scope 不串项目；
- 注入可追溯并一键撤销；
- MemoryJudge 只看当前请求，不看记忆库；
- 删除与彻底遗忘可验证。

## 10. Phase 8 — 需求压缩副模型与高级上下文

### 目标

在核心省钱机制已经可测后，增加用户自选的需求压缩，不把额外模型调用当作默认优化。

### 交付

- RequirementReducer 独立 ExecutionTarget；
- 本地长度、内容类型、隐私和 break-even gate；
- 固定短输出合同；
- 原始请求权威与可见；
- 净节省结算；
- 可选 ContextCompactor，默认关闭。

### 验收

- 短请求零调用；
- 代码、数字、路径、否定和约束保留；
- 不缩减最终回答；
- 失败回退原始输入；
- 只有净节省为正的调用算成功；
- 开关互相独立。

## 11. Phase 9 — 公平对标与发布候选

### 目标

证明产品达到成本、质量、速度和 UI 标准。

### 交付

- 冻结版本的 KnightFrame 与 Claude Code；
- 完整任务 manifest、原始运行记录、账单和评分；
- Windows 安装包、签名、SBOM；
- 干净 VM/实体机回归；
- 中英文视觉与交互审核；
- 升级/卸载/损坏包检测；
- 限制与已知问题文档。

### 发布门禁

- 同模型、同任务、同质量实际费用节省 ≥20%；
- 综合评分不低于对手；
- 完成度不低于对手；
- 无 P0/P1 数据破坏或权限逃逸；
- 安装、更新、卸载无阻塞；
- 所有 `shipped` 功能满足 Trigger/Receipt/Metric/Acceptance；
- UI 无遮挡、偏蓝、硬编码文案、重复 logo 或明显视觉失衡。

## 12. Benchmark 设计

### 12.1 公平条件

每个配对任务固定：

- 仓库 commit/snapshot；
- Windows 镜像与硬件资源；
- 主模型、provider、endpoint、reasoning effort、temperature；
- 网络环境与速率限制；
- 用户 prompt；
- 权限；
- 时间上限；
- 成功判定和测试；
- 冷/热缓存条件；
- agent 每个不同任务只运行一次。

不要求同一任务反复运行；用更宽的任务集合抵抗偶然性。若 provider 非确定性引发明显异常，只能按预先写明的 invalid-run 规则剔除，不能选择性重跑赢家。

### 12.2 任务集

至少覆盖：

1. 精确单字符修复；
2. 跨文件 bug；
3. 配置追踪；
4. 调用链影响修改；
5. 新增小功能；
6. 测试失败诊断；
7. 构建错误；
8. 重构/重命名；
9. 多语言项目；
10. 长需求；
11. 大日志/MCP 输出；
12. 外部文件变化与恢复；
13. 中文路径和中文需求；
14. 权限阻塞；
15. 缓存冷态与热态连续任务。

### 12.3 评分

```text
final = completion*0.50 + token_efficiency*0.30
      + speed*0.10 + flexibility*0.10
```

- 完成度由自动测试、文件范围、需求满足和人工盲审组成；
- token 效率使用 provider usage，区分 fresh/cache-read/cache-write/output；
- 速度从用户发送到可验证完成，另报 TTFT；
- 灵活度包含开源模型、工具协议差异、错误恢复与用户 steer；
- 费用门禁独立于 token 分数，以真实账单为准。

### 12.4 报告

必须报告：

- 成功/失败/超时；
- 主模型与副模型调用数；
- fresh/cache read/cache write/output/reasoning token；
- 实际费用与来源；
- 工具调用分布、读取字节、edit 变更字节；
- 项目图查询与避免的 broad read；
- TTFT、总时长；
- 质量证据；
- 冷/热态；
- 任何 fallback 或 protocol degradation。

不使用外部项目宣传数字代替 KnightFrame 结果。

## 13. 测试层次

| 层 | 内容 |
|---|---|
| 单元 | 状态机、坐标、parser、reducer、权限、i18n |
| 属性 | SSE 任意切块、Unicode edit、路径规范化、schema |
| Golden | 图谱符号/关系、工具投影、需求压缩、记忆 |
| 集成 | provider fixture、SQLite generation、Tauri event |
| E2E | 真实代码任务、权限、取消、恢复 |
| 打包 | 签名 installer、更新、卸载、终端静默 |
| 视觉 | 中英/DPI/窗口/动效/遮挡/中性色 |
| 安全 | 路径逃逸、命令注入、进程树、密钥泄漏 |
| 性能 | 首建、增量、查询、IPC、内存、启动 |
| 成本 | 同任务账单、缓存、工具和副模型 break-even |

## 14. 每阶段反模式检查

- 是否为了赶阶段只做了 UI，后端没有 producer？
- 是否接受了 schema 字段但不执行？
- 是否新增常驻 prompt/tool token？
- 是否让副模型默认调用？
- 是否把本地全量资料库误缩减？
- 是否出现一行 edit 重传大段文本？
- 是否用估算或不同模型宣称省钱？
- 是否漏掉 Windows 打包版？
- 是否硬编码了中英文字符串？
- 是否引入没有明确许可证和来源的代码/资产？

任一回答为“是”时阶段不得标完成。
