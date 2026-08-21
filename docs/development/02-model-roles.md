# 02 — 主模型与副模型

## 1. 核心原则

KnightFrame 不把“模型”视作一串 model ID，而把每次调用绑定到明确的物理执行目标：

```text
ExecutionTarget
  provider_profile
  physical_adapter
  model_id
  endpoint
  auth_reference
  capabilities
  context_limit
  output_limit
  pricing_source
  cache_policy
  privacy_class
```

不能根据 `model_id` 猜供应商。相同名称可能存在于本地服务、OpenRouter、官方 API 或兼容网关，它们的认证、流式事件、工具调用、reasoning、缓存和价格都可能不同。

## 2. 角色边界

### 2.1 主模型 `Primary`

每个会话只有一个当前主执行目标。主模型拥有：

- 用户原始请求；
- 当前任务目标和必要约束；
- 被本地逻辑选中的 Skill；
- 被选中的少量记忆；
- 项目速查与工具；
- 工具模型投影；
- 最终回答权。

主模型不得：

- 直接写长期记忆；
- 读取整个记忆库或 Skill 目录；
- 获得内部指标账本、长 ID 或 artifact 全文；
- 因副模型建议而失去对原始用户意图的访问；
- 自行切换 provider 或主模型。

### 2.2 副模型 `Auxiliary`

副模型不是副代理。它不进入主代理循环，不持有项目工具，不编辑文件，不产生最终回答。它只能执行用户为某项功能单独开启的窄任务。

第一阶段允许的副模型职责：

| 角色 | 默认 | 输入 | 输出 | 禁止 |
|---|---|---|---|---|
| `RequirementReducer` | 关闭 | 当前用户请求 | 短需求简报 | 回答问题、改变原意、调用工具 |
| `SkillSelector` | 关闭 | 当前请求 + 本地候选 Skill 摘要 | 候选排序 | 读取 Skill 全库、注入正文 |
| `MemoryJudge` | 关闭 | 当前请求 | 是否需要记忆 + 极短检索词 | 写记忆、读取全库 |
| `ContextCompactor` | 关闭 | 明确选定的旧会话片段 | 结构化续接摘要 | 修改当前任务、覆盖原始历史 |

这四项配置互相独立。用户开启需求压缩，不等于开启 Skill、记忆或上下文压缩副模型。

群聊、副代理和复杂模型路由第一阶段不实现；架构仅保留未来角色枚举，不能因此提前注入 schema 或 UI。

## 3. 默认路径：零副模型调用

当所有副模型功能关闭时：

```text
用户请求
  -> 本地长度与语言检测
  -> TaskManager
  -> 本地 Skill 关键词匹配
  -> 若记忆已开启：本地记忆关键词匹配
  -> 上下文编译
  -> 主模型
```

必须满足：

- 不解析副模型 provider；
- 不加载副模型 prompt；
- 不发送副模型能力说明给主模型；
- 不预热或保活副模型；
- 不产生副模型 token、网络请求和等待时间；
- UI 明确显示这些功能为关闭，但不重复弹窗打扰。

## 4. 需求压缩的正确语义

### 4.1 压缩对象

`RequirementReducer` 只压缩**用户的当前要求**，不是压缩主模型的最终回答，也不是自动总结整个会话。

它适用于长、重复、口语化但目标明确的输入。短输入直接跳过，即使功能已开启。

### 4.2 权威来源

- 原始用户输入始终保存在本地会话中并在 UI 可见；
- TaskManager 的用户目标以原始输入为权威；
- 副模型产生的短简报是上下文辅助，不替换原文，不回写用户消息；
- 主模型可同时收到原始请求的必要部分和短简报；对于超长输入，可按结构化附件引用原始内容，而不是复制两遍；
- 用户修改要求后，旧简报失效，不可复用。

### 4.3 触发

全部条件同时满足才调用：

1. 用户在该功能设置中选择了副模型执行目标；
2. 功能开关开启；
3. 输入超过可配置的净收益门槛；
4. 输入不是主要由不可改写的代码、日志、表格或精确引文组成；
5. 预计节省的主模型输入费用大于副模型费用与延迟预算；
6. 未命中隐私策略禁止外发；本地副模型不受外发限制。

门槛由本地计算，不调用模型判断是否值得调用模型。

### 4.4 输出合同

副模型只输出固定短结构：

```text
goal
constraints[]
deliverables[]
non_goals[]
open_questions[]
```

字段缺失允许为空；不得输出解释、客套、思维过程或新的建议。最大输出 token 写入角色配置，并在调用前强制。

### 4.5 回执

UI 在本轮度量区显示：

- 是否执行；
- 使用的副模型；
- 原始估算 token 与简报 token；
- 副模型费用与延迟；
- 预计/结算净节省；
- 跳过或失败原因。

该回执不进入主模型 prompt。

## 5. Skill 与记忆的副模型判断

### 5.1 默认本地匹配

- Skill：名称、描述、显式触发词、语言、文件类型和任务类型的本地索引；
- 记忆：层级、范围、关键词、实体、项目和时间的本地索引；
- 中文短查询使用 FTS + 子串/前缀补充；
- 默认不做 embedding，不调用 LLM。

### 5.2 可选副模型模式

只有用户分别开启 `SkillSelector` 或 `MemoryJudge` 时：

1. Skill 仍由本地索引先筛到很小候选集；副模型只做候选排序；
2. MemoryJudge 先看当前请求，只回答是否需要记忆以及极短检索词；
3. 本地记忆索引再用该检索词取候选，副模型看不到完整记忆库；
4. 运行时按预算取 Skill 正文或少量记忆；
5. 副模型失败立即回退本地关键词路径，不阻断主任务。

禁止把整个 Skill 或记忆目录发送给副模型。禁止让副模型“自己随便找”。

## 6. SkillOpt 的模型边界

SkillOpt 保留，但它不是每轮 Skill 路由器。它在离线或低优先级时段分析：

- 哪些本地关键词带来了实际 Skill 使用；
- 哪些 Skill 总被选中但未被主模型采用；
- 哪些描述过长、冲突或重复；
- 哪些中英文触发词缺失。

默认 SkillOpt 使用确定性统计。若未来允许用户为 SkillOpt 配置副模型，该调用必须：

- 不在用户 Turn 热路径；
- 不读取项目源代码或会话秘密；
- 只输出待用户确认的建议；
- 不自动修改 Skill；
- 有独立费用和隐私回执。

## 7. 物理隔离

### 7.1 独立客户端

主模型与每个副模型角色拥有独立：

- `ExecutionTarget`；
- HTTP/client 配置；
- 并发信号量；
- 速率与费用预算；
- provider 会话/cache 标识；
- 超时与最大输出；
- 日志和 usage 归属。

副模型不能继承主模型客户端后只替换字符串 `model_id`。若目标 provider adapter 无法明确绑定指定模型，配置验证失败并提示用户。

### 7.2 独立上下文

- 主模型上下文不包含副模型 prompt；
- 副模型上下文不包含主会话完整历史；
- 副模型输出作为一条有来源的动态尾部片段进入主模型；
- 两者的供应商缓存分别结算；
- 切换副模型不使主模型稳定前缀失效；
- 副模型错误不写成主模型消息。

### 7.3 独立预算

每个角色配置：

```text
max_input_tokens
max_output_tokens
max_cost_per_call
max_latency_ms
max_calls_per_turn
privacy_mode
fallback
```

第一阶段默认 `max_calls_per_turn = 1`。不得用副模型重试循环吞掉省下的费用。

## 8. 调用算法

```text
resolve feature toggle
  -> if off: emit AuxRunSkipped(off), return
resolve physical ExecutionTarget
  -> if invalid: emit AuxRunSkipped(config_invalid), return local fallback
run deterministic break-even and privacy gate
  -> if false: emit AuxRunSkipped(reason), return
reserve auxiliary quota
  -> if main work waiting: defer or skip
build role-specific minimal input
call provider once
validate small output contract
  -> invalid: discard, return local fallback
emit AuxRunFinished(usage, cost, latency, before, after)
append accepted result to dynamic context tail
```

所有 `Skipped` 回执只存本地；正常情况下 UI 使用紧凑图标/提示，不输出大量日志。

## 9. 主模型切换与副模型切换

- 主模型切换只在 Turn 边界生效；进行中的 Turn 不热切换；
- 切换主模型会建立新的 provider/cache 轨道，并显示缓存断裂回执；
- 副模型配置切换不改变主模型工具 schema、system prompt 或历史；
- 主模型不可自动降级到副模型；
- 副模型 fallback 链必须是同一角色的明确配置，可设为“失败即本地逻辑”；
- OpenCode 免费模型可作为用户选择的默认推荐 profile，但首次运行必须探测实时可用性，不能把免费状态或模型 ID硬编码为永远有效。

## 10. 能力矩阵

每个执行目标在首次配置或模型变化时探测并记录：

| 能力 | 主模型 | 副模型 |
|---|---:|---:|
| 流式文本 | 必须 | 可选 |
| 工具调用 | 必须或明确降级 | 禁止使用 |
| reasoning summary | provider 支持时展示 | 不展示 |
| prompt cache usage | 记录 | 记录 |
| structured output | 优先 | 需求压缩等角色优先 |
| images | 按模型 | 第一阶段不用 |
| 最大上下文/输出 | 探测并验证 | 探测并限制 |

未知能力不得静默假定为支持。

## 11. 可观察性与成本归属

每次模型调用记录：

- role；
- provider profile 与物理 adapter；
- model；
- fresh input、cache read、cache write、output/reasoning token；
- provider 报告费用或估算来源；
- 首 token、总耗时；
- 成功、错误或 fallback；
- 对主任务的净节省结算。

UI 默认展示合计，可展开主模型/副模型明细。估算必须标“估算”，不能混成实际账单。

## 12. 验收

### 12.1 关闭路径

- 所有副模型功能关闭时，网络记录中副模型请求数严格为 0；
- 主模型 prompt 不出现副模型角色、prompt 或配置；
- 不加载副模型权重、不创建外部客户端、不发 keepalive；
- 性能相较删除副模型代码路径无显著差异。

### 12.2 需求压缩

- 短请求零副模型调用；
- 代码、路径、数字、否定、约束和交付物的保留率达到冻结测试集要求；
- 原始消息永远可见、可导出；
- 副模型失败不阻断主任务；
- 只有净节省为正的调用计入成功率；
- 不对最终回答做二次缩减。

### 12.3 Skill/记忆选择

- 默认只执行本地匹配；
- 开启副模型后仍先本地筛选；
- 副模型最多看到限定候选摘要；
- 关闭任一功能只影响该功能，不隐式关闭或开启其他功能；
- 每次注入都有来源、token 和使用结果回执。
