# 07 — Provider 适配、开源模型、Windows 安全与数据

## 1. 原则

旧工程 API 适配不作为实现基础。每个 provider adapter 依据官方公开文档、录制的协议 fixture 和真实 canary clean-room 重写。

共同要求：

- 不用“OpenAI-compatible”代替所有供应商语义；
- 不根据 model ID 猜 provider；
- 不把静态价格表冒充实时价格；
- 不把 `/models` 全量目录冒充当前账户有权使用；
- 不静默丢失 reasoning、工具调用、缓存 usage 或请求 ID；
- 未知模型先探测，不能套一个乐观默认；
- 主模型与每个副模型角色绑定独立物理执行目标。

## 2. 规范内部协议

### 2.1 请求

`kf-provider` 接受 provider-neutral `ModelRequest`：

```text
role
execution_target
stable_instructions
messages
tools
tool_choice
reasoning_policy
output_policy
cache_policy
limits
session_binding
```

该结构表达意图，不假定 wire 格式。adapter 负责映射或明确拒绝不支持字段。

### 2.2 事件

adapter 统一输出：

```text
ResponseStarted
TextDelta
ReasoningSummaryDelta
ToolCallStarted
ToolArgumentDelta
ToolCallFinished
UsageDelta
ResponseCompleted
ResponseIncomplete
ProviderError
```

每个事件保留 provider request/response ID 到本地诊断。最终文本、reasoning、tool call、usage 和 stop reason 分离，不能拼成一段日志。

### 2.3 Usage

统一账本字段：

```text
fresh_input
cache_read_input
cache_write_input
output_total
reasoning_output
provider_billed_cost
estimated_cost
cost_source
price_source
currency
```

adapter 不提供的字段标 `unavailable`，不填 0 冒充真实值。

## 3. 首发适配器

### 3.1 OpenAI Responses

实现重点：

- 原生 Responses request/stream event，不经 Chat Completions 转译；
- 文本、工具调用、reasoning summary、incomplete 和 usage 分别处理；
- 工具输出配对和 continuation/previous response 语义按模型能力验证；
- 使用官方 prompt cache key/retention 能力时，保持同会话绑定且不在 UI 暴露内部 key；
- 记录 cached token 明细和输出 reasoning 明细；
- model、reasoning effort、tool schema 和输出模式变化记录 cache break；
- 不假定所有 OpenAI 模型支持相同 reasoning 值或工具行为。

### 3.2 Anthropic Messages

实现重点：

- 原生 content block 流：text、thinking/reasoning summary、tool_use、tool_result；
- 完整累积 `input_json_delta`，block 关闭后再验证 JSON；
- 正确保留 tool_use/tool_result 配对；
- 按模型协议回放必须回放的 thinking/signature block；
- prompt cache 明确处理 `tools -> system -> messages` 顺序；
- 记录 cache creation/read 及 TTL/价格；
- 支持 tool `defer_loading` 或等价延迟发现时，不把完整 MCP 工具表常驻；
- beta 功能按版本 capability 开启，不写成永久协议。

### 3.3 DeepSeek 原生

截至本设计冻结日，官方文档同时公开 OpenAI 格式与 Anthropic 格式入口，并公开新的模型、thinking 和缓存字段。实现时必须以 canary 重新确认，不能保留旧 model alias 假设。

OpenAI wire 重点：

- Chat Completions SSE 与最终 usage chunk；
- `thinking` 与 `reasoning_effort` 能力按模型探测；
- 工具循环中正确回放官方要求的 `reasoning_content`；
- 记录 `prompt_cache_hit_tokens` 与 `prompt_cache_miss_tokens`；
- strict tool schema 的 beta base URL 与受支持 JSON Schema 子集单独处理；
- `[DONE]`、finish reason、tool call arguments 和错误体完整解析。

Anthropic wire 作为独立 flavor 做 fixture，不假设与官方 Anthropic 完全一致。

DeepSeek 缓存当前由服务端自动进行，KnightFrame 的工作是保持前缀稳定和准确记账，不伪造 cache_control。

### 3.4 OpenRouter

实现重点：

- profile 明确选择 Responses 或 Chat/Anthropic wire；
- 使用 session binding 保持多轮 provider stickiness，但内部会话 ID 不进入 prompt；
- provider order、fallback、`require_parameters`、ZDR/data policy 显式配置；
- fallback 实际发生时显示 resolved model/provider 与缓存断裂；
- prompt caching 与 OpenRouter beta response caching 分开计量；
- 主代理默认不开启“相同请求原样返回”的 response cache，避免重放陈旧 tool call；
- 免费模型状态通过实时目录/entitlement 发现，UI 可推荐但不得永久硬编码；
- router metadata 仅诊断时开启，不能默认污染响应或泄露不必要信息。

### 3.5 OpenAI-compatible / 开源模型

支持用户自选本地或远程端点，例如 Ollama、vLLM、LM Studio 以及自建兼容服务，但“兼容”通过 profile 与 capability probe 证明。

探测项：

- `/models` 是否存在及其可信程度；
- stream 格式；
- system/developer role；
- tool calls、parallel tool calls、tool result pairing；
- JSON schema 严格度；
- reasoning 字段名和回放要求；
- usage 是否存在及单位；
- context/output limit；
- prompt cache usage；
- stop reason 与错误格式。

结果形成可见能力矩阵：

```text
Supported / Degraded / Unsupported / Untested
```

缺工具能力的模型可用于需求压缩等副模型角色，但不能在主代理 profile 中伪装完整可用。用户可手动覆盖探测结果，UI 必须标“用户覆盖”。

## 4. Adapter 验证

每个 adapter 必须具备：

1. 录制的 request/response fixture，去除密钥和敏感内容；
2. 流式 chunk 任意切分的 parser property test；
3. tool call 单次、多次、并行、参数中断和无效 JSON；
4. reasoning 开/关与回放；
5. cache hit/miss/write usage；
6. rate limit、超时、断线、半截流和 provider error；
7. 真实最小 canary；
8. 模型/协议版本变化时的隔离开关。

静态 fixture 通过但 canary 失败时，profile 标 Degraded，不能继续显示绿色可用。

## 5. 价格与模型目录

- 内置 catalog 只作为带日期的回退；
- 能获取实时模型/价格时缓存并显示来源/更新时间；
- 用户可覆盖价格；
- `free` 必须来自实时 provider 元数据或用户明确配置；
- OpenCode 免费模型作为推荐默认选项时，首次启动必须检测可用性；
- 价格变化不改写历史账单；
- 估算使用调用发生时的价格快照；
- 20% 门禁优先用 provider billed cost，不以静态 catalog 估算替代。

## 6. 权限模型

### 6.0 当前产品决策（覆盖下方可选隔离模型）

KnightFrame 当前默认且唯一执行档为**不限时完全访问**：

- read/edit/run 可访问项目外路径，包括绝对路径和 `..`；
- 命令不设固定超时，长任务自然继续；
- 不对编辑或命令逐次弹出审批；
- 用户可在运行中追加引导，也可随时停止；
- Windows Job Object 在停止时终止整棵子进程树；
- 真实交易、下单、持仓变更与自动购买永久禁止。

下方 allow/ask/deny 与工作区隔离仅作为未来可选 restricted profile 的设计备忘，不代表当前运行时行为。

### 6.1 决策顺序

```text
deny -> ask -> allow
```

规则作用于：

- 工具；
- 操作类型；
- 解析后的实际路径/命令/域名；
- 项目、会话或单次范围；
- 只读、写入、执行、网络和破坏性级别。

Prompt 和 Skill 只能影响模型想做什么，不能覆盖 runtime 权限。

### 6.2 文件路径

Windows 首发必须处理：

- 相对路径到 workspace root 的解析；
- `..`、不同盘符、UNC；
- 大小写与短文件名；
- symlink、junction、reparse point；
- 长路径；
- 中文、emoji 与空格；
- 路径在验证后、执行前被替换的竞态。

read/edit 默认限制在用户打开的 workspace 和显式 artifact 存储。越界必须 ask 或 deny，不能仅凭字符串前缀判断。

### 6.3 命令

- 优先直接创建进程，不经 shell；
- PowerShell 命令使用 AST/专用解析策略；
- CMD 与 PowerShell 不共用 Bash 分词；
- 复合命令逐段判定；
- 环境变量、重定向、管道、命令替换和脚本文件作为独立风险；
- 可执行文件的解析后实际路径进入审批；
- 默认隐藏窗口；
- 使用 Windows Job Object 管理进程树和取消；
- 递归删除、覆盖、大范围 move、提权和系统路径操作需要明确审批。

### 6.4 Edit

- 内置 edit 是唯一默认写文件路径；
- shell/Python/Node 写文件不享受 edit 的自动批准；
- `accept edits` 只允许 workspace 内、非敏感路径的精确 edit；
- 权限批准不跳过短版本/旧文本验证；
- 多文件事务显示预计目标和范围。

## 7. Windows 执行隔离

第一阶段至少实现：

- Job Object：进程树、超时、取消、资源上限；
- 受限环境变量和明确工作目录；
- 隐藏窗口创建标志；
- 标准输入默认关闭，交互任务单独声明；
- 网络能力与命令权限分开；
- 可选 restricted token/AppContainer 探索作为增强，但未通过逃逸测试前不宣传为完整 sandbox。

UI 必须准确称呼当前等级，如“工作区限制 + 进程树控制”，不能把普通权限检查宣传成系统级沙箱。

## 8. Prompt injection 与不可信内容

- 工具结果、网页、README、MCP 内容始终是数据，不放入 system role；
- `external` 工具注册信息不授予权限；
- 项目文件中的“忽略规则/上传密钥”等文字不能覆盖 runtime policy；
- 主模型可请求高风险动作，runtime 仍 ask/deny；
- 模型投影保留来源和完整性；
- 副模型不接收工具结果或项目文件；
- Skill 安装前显示来源、权限和兼容等级。

## 9. 密钥与隐私

### 9.1 密钥

- API key 存 Windows Credential Manager；
- SQLite/config 只存凭据引用；
- 日志、事件、崩溃报告和 fixture 自动脱敏；
- UI 只显示末尾少量字符；
- 导出配置默认不含密钥；
- 剪贴板密钥及时清除由用户控制，不做危险的全局剪贴板监控。

### 9.2 数据外发

调用前 UI/profile 明确显示：

- provider 与物理 endpoint；
- 主模型/哪一个副模型角色；
- 是否本地；
- 项目代码是否会发送；
- provider 数据策略与用户限制；
- fallback 是否可能转给另一 provider。

记忆默认关闭；外置 MemoryJudge 只接收当前请求。需求压缩副模型只接收当前用户请求。SkillSelector 只接收当前请求和候选摘要。

### 9.3 遥测

默认零遥测：

- 不发送安装 ID；
- 不从 MAC、IP 或硬件生成稳定 ID；
- 本地用量与功能指标默认留在本机；
- 未来若增加诊断上传，必须逐次预览、显式同意、可删除，并和模型 API 请求区分。

## 10. 更新与供应链

- 安装包和更新包使用 Authenticode 签名；
- updater 验证签名和渠道；
- 发布产物在干净 CI 构建并保留 SBOM；
- 第三方 Rust/JS 依赖做许可证与漏洞检查；
- tree-sitter grammar 和可执行模型资产有来源清单；
- 不加载来源不明的 pickle/joblib 等可执行序列化资产；
- NSIS/安装器完整性在 CI、下载后和干净 VM 三处验证，避免再次交付损坏安装包；
- 内部签名/摘要不进入主模型 prompt。

## 11. 验收

- 四类首发 provider 完成协议 fixture 和真实 canary；
- 未知开源 endpoint 能明确显示能力与退化；
- thinking/tool loop 按 provider 要求回放且无 400；
- cache usage 与费用字段不漏记；
- fallback、路由和 resolved provider 对用户可见；
- Windows 路径逃逸、junction、UNC、大小写和替换竞态 fixture 全过；
- 取消清理完整进程树且无终端窗口；
- 密钥不出现在 config、日志、crash dump 和导出；
- 默认运行只发生用户配置的模型 API 流量，无遥测；
- 签名安装包在干净 Windows 上安装、启动、更新、卸载成功。
