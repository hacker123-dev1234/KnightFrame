# 03 — 全量项目速查与代码图谱

## 1. 定位

项目速查是 KnightFrame 最重要的省钱设施，不是附属可视化。它必须让主模型用一个短词定位到：

- 哪个文件、哪一行、哪个符号；
- 完整签名和精确代码范围；
- 谁定义、谁引用、谁调用、调用了谁；
- 相关测试、配置、路由、资源和依赖；
- 修改后的影响范围；
- 当前数据是否新鲜。

模型不应为了回答这些问题反复执行目录枚举、全库 grep 或整文件 read。

## 2. “全量”的严格定义

项目本地速查必须全量，查询给模型的结果必须精简。两者不能混淆。

### 2.1 文件覆盖

首次构建结束时，每个未被用户忽略规则排除的文件都有记录：

- 项目相对路径；
- 文件类型、语言、大小、更新时间和当前短版本；
- 是否可解析、解析器版本、失败原因；
- 文本搜索状态；
- 所属模块、包、构建目标或资源分类；
- 与其他文件的 import、生成、测试、配置等关系。

不支持 AST 的文件也必须进入清单与文本索引，不能从项目速查中消失。

### 2.2 代码覆盖

对受支持语言，索引必须覆盖：

- 模块、命名空间、包；
- 类、结构体、接口、trait、enum、type；
- 函数、方法、构造器、属性、字段、常量；
- 参数、返回类型、泛型和可见性；
- 定义范围、名称范围、签名和父符号；
- import/use/include；
- 调用、引用、继承、实现、类型使用；
- 测试与被测符号；
- 语言适配器能可靠提取的路由、事件、配置和资源。

### 2.3 非代码覆盖

- TOML、JSON、YAML、XML、properties、Gradle/Maven/Cargo/npm 配置；
- Markdown 和项目规则文档的标题与锚点；
- Svelte/HTML/CSS 中的组件、样式和资源引用；
- 测试数据、模板、提示词与 locale key；
- 文件生成关系和构建输出边界。

二进制文件只记录元数据与引用，不读取内容进入模型。

### 2.4 语言支持声明

“项目 Ready”不等于所有语言都获得同等语义精度。设置页必须显示语言覆盖矩阵：

```text
Full      AST + 符号 + 主要关系 + 增量更新
Partial   AST/文本符号，但部分关系缺失
Text      文件和全文检索，无可靠符号图
Ignored   用户规则排除
Failed    应解析但失败
```

首个可发布版本优先完整支持 Rust、Kotlin、Java、Python、JavaScript、TypeScript、TSX、Svelte、Go、C、C++、C#、PowerShell，以及常见配置格式。实际发布范围由 golden corpus 验收决定，未通过的语言不得标 `Full`。

## 3. 项目进入门

### 3.1 首次打开

代码代理进入工作区前先完成可用索引：

```text
发现文件 -> 分类 -> 解析 -> 建立关系 -> 构建搜索 -> 校验 -> Ready
```

UI 显示：

- 当前阶段；
- 已处理/总文件数；
- 当前语言或模块；
- 节点、关系、失败文件和预计剩余时间；
- 可取消；
- 完成覆盖率与耗时。

在 `Ready` 前：

- 可进行普通聊天与设置；
- 代码任务排队等待，不能用半成品图谱做确定性回答；
- 用户可选择“仅文本模式继续”，该降级必须有醒目回执；
- 取消后半成品 generation 不得标记为 Ready。

### 3.2 再次打开

- 无变化：直接装载上一个完整 generation；
- 少量变化：先装载完整 generation，将变更区域标 Updating 并增量更新；
- Git 分支/工作树完全变化：创建新 generation；
- schema、解析器或 ignore 规则变化：按影响范围迁移或重建；
- 任何情况下不得混用不兼容 generation。

## 4. 数据模型

### 4.1 短 ID

模型和 UI 使用短 ID：

```text
P3    package/module
F18   file
S42   symbol
C7    configuration item
T9    test
R12   relation/result
```

ID 只在当前项目 generation 内稳定；数据库内部可以有更完整的身份与变更指纹，但不进入模型上下文。

### 4.2 节点

最小统一节点：

```text
Project
Package/Module/Folder
File
Symbol
Test
Config
Route/Endpoint
Resource
DocumentSection
```

`Symbol` 通过 `kind` 区分 class/function/method/interface/enum/type/field/constant 等，避免为每种语言制造大量表和工具枚举。

每个符号至少保存：

- short ID；
- 名称与 qualified name；
- kind、language；
- file ID；
- definition span、name span、body span；
- 极短签名；
- parent ID；
- visibility；
- file version 与 generation；
- parser confidence。

### 4.3 关系

统一关系集合：

```text
CONTAINS / DEFINES / MEMBER_OF
IMPORTS / DEPENDS_ON
CALLS / REFERENCES / USES_TYPE
INHERITS / IMPLEMENTS / OVERRIDES
TESTS
READS_CONFIG / WRITES_CONFIG / CONFIGURES
HANDLES / TRIGGERS / PUBLISHES / CONSUMES
ROUTES_TO / HTTP_CALLS
GENERATES / USES_RESOURCE
CHANGES_WITH
```

关系只有在存在生产者、fixture 和质量指标时才可声明为支持。不能重演“schema 写了 OVERRIDES，解析器从未产生”的无效功能。

每条关系保存：

- source/target；
- 关系类型；
- 证据位置；
- 方向；
- 来源解析器；
- 置信等级；
- generation。

静态可能路径与真实运行时观测必须分开标记。普通 BFS 可达集合不得伪装为真实执行顺序。

## 5. 构建管线

### 5.1 文件发现

- 使用原生 Rust walker；
- 合并项目 ignore、VCS ignore 和用户设置；
- 默认排除构建输出、依赖缓存、大型二进制和 `.git`；
- 排除原因可查；
- Windows junction、symlink、大小写和循环必须安全处理；
- 发现结果稳定排序，保证可重复构建。

### 5.2 解析

- tree-sitter 提供离线、快速、可增量的语法基线；
- 语言适配器把不同 AST 映射到统一节点与关系；
- LSP 作为可选增强，补全跨文件引用、类型和重命名信息；
- LSP 不可用时仍可完成项目 Ready，只降低相应关系等级；
- 单文件解析失败不阻断其他文件，但必须计入覆盖率并显示。

### 5.3 连边

分两阶段：

1. 文件内定义、成员、局部引用；
2. 项目级名称解析、imports、calls、tests、config 和资源关联。

名称歧义不能静默选择第一项。结果保存多候选及置信度，模型查询时返回歧义列表或要求更精确条件。

### 5.4 搜索

索引字段至少包括：

- symbol name；
- qualified name；
- path；
- signature；
- config key；
- route；
- document heading；
- 文本 token。

排序优先级：精确名称 > 精确 qualified name > 前缀 > 标识符分词 > 路径 > FTS > 子串。中文与短字符串在 FTS 之外保留子串/前缀补充。

默认不启用 embedding。以后如增加语义检索，必须是用户显式开启、可显示相对词法检索的真实增益，并能完全关闭。

## 6. 增量新鲜度

### 6.1 触发源

- KnightFrame 内置 edit 成功；
- 外部编辑器保存；
- 文件新增、删除、移动；
- Git checkout、merge、rebase；
- 构建或代码生成；
- 依赖清单、解析配置、ignore 规则变化。

### 6.2 更新策略

```text
收集稳定后的变更集合
  -> 解析变更文件
  -> 重建直接关系
  -> 重新解析受影响依赖者
  -> 构建候选 generation
  -> 校验
  -> 原子切换 current generation
```

不得逐文件提交一个对外可见的混合快照。较大更新期间，查询基于上一个完整 generation，并对命中变更区域返回 `stale`。

### 6.3 新鲜度状态

```text
Ready      当前完整
Updating   有已知变更，正在生成下一代
Stale      变化无法可靠归入当前图谱
Failed     更新失败且没有可安全使用的结果
```

UI 常驻显示紧凑状态；展开后显示最后更新时间、变更文件数、错误和覆盖率。

模型查询结果必须带短 `snapshot` 与 `freshness`，但不带内部指纹。

## 7. 面向模型的查询

项目速查不把 20–30 个 MCP 工具全部暴露给模型，只通过核心 `project` 工具提供四种操作：

```text
project.find(q, kind?, path?, limit?)
project.show(id, code?, links?, tests?)
project.links(id, relation?, direction?, limit?)
project.impact(id|changes, depth?, limit?)
```

### 7.1 `find`

默认返回：

```text
S42 parse_config  fn(path)->Config  F18:73  in:2 out:4 tests:1
```

要求：

- 默认 5 个结果；
- 返回命中总数和省略数；
- 精确匹配优先；
- 歧义明确；
- 不返回整个函数体；
- 可直接把 ID 交给 `show/read/edit`。

### 7.2 `show`

默认返回符号身份、精确范围、签名、父级、关系计数和新鲜度。只有 `code=true` 才返回限定代码范围。

### 7.3 `links`

默认返回 `file:line + symbol + relation`，支持 callers/callees/references/tests/imports 等关系过滤。必须提供总数、当前页和继续参数。

### 7.4 `impact`

用于修改前的影响分析：

- 区分“依赖它”和“它依赖”；
- 距离衰减和关系权重可配置；
- 返回最可能受影响的符号、文件和测试；
- 输出是静态预测，不声称为真实执行路径；
- 验收真值来自独立历史/人工标注，不由同一张图自证。

## 8. Graph-first 策略

主模型的短规则只说明：

```text
符号、调用、引用、影响问题先用 project；拿到精确范围后再 read/edit。
```

运行时通过工具可见性和结果设计引导，不依靠长 prompt 强迫模型“忘掉预训练”。

默认不暴露单独的 `ls`、`glob`、`grep` 工具：

- 文件路径查询走 `project.find(kind=file)`；
- 符号与文本定位走项目搜索；
- 精确内容走 `read`；
- shell 只在 `run` 中作为受控兜底，并标记为何图谱无法满足。

小仓库、明确路径、单文件简单修改可以直接精确 `read`，不机械增加一次图查询。

## 9. 给人看的项目视图

项目速查也提供 UI，但 UI 不是功能完成证据：

- 全局搜索；
- 符号详情；
- 调用/引用/测试关系；
- 改动影响；
- 解析覆盖与失败；
- 索引进度与新鲜度。

默认展示局部子图或关系列表，不渲染整个项目的“毛线团”。可视化只消费查询 API，不维护另一份图数据。

## 10. 指标

### 10.1 构建

- 首建 p50/p95；
- files/s；
- 解析成功率；
- Full/Partial/Text/Failed 覆盖；
- 节点、关系与数据库大小；
- 重开复用时间；
- 增量更新延迟和增量/全量比。

### 10.2 查询

- exact top-1；
- top-k recall、MRR；
- 引用 precision/recall；
- warm p50/p95；
- 默认模型投影 token；
- 图命中后仍发生的 broad read/grep 次数；
- 歧义、过期和 fallback 比率。

### 10.3 省钱

- 避免的目录枚举次数；
- 避免的整文件 read 字节/token；
- 每个成功任务的图查询 token 与后续工具 token；
- graph-first 与禁用图谱的同任务成本差；
- 图谱等待时间是否抵消节省。

## 11. 验收

### 11.1 覆盖

- fixture 中每个未忽略文件都有记录；
- 支持语言的定义范围与符号身份达到 golden 标注；
- 不支持语言明确为 Text，不伪装 Full；
- 解析失败文件可定位并有原因。

### 11.2 定位与关系

- 精确标识符查询 top-1 目标不低于 99%；
- 已发布语言的引用 recall 目标不低于 95%；
- warm 精确查询 p95 目标低于 100ms；
- 默认返回目标控制在 300 model tokens 内；
- 无结果区分真无结果、索引过期、解析失败、被忽略和查询超时。

### 11.3 更新

逐项验证：改名、移动、删除、跨文件调用、Git 切换、外部保存、生成文件、中文路径、大小写、junction。任何命中过期文件的查询不得悄悄返回旧位置。

### 11.4 任务节省

在符号定位、影响分析、配置追踪和测试定位任务上，对比熟练的 grep/read agent，而非“读取全库”的稻草人基线。必须记录相同任务成功质量与真实 token。
