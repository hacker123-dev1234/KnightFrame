# 04 — 极简工具、上下文与缓存

## 1. 目标

工具层必须让模型少选、少写、少读，同时不能因过度压缩而失去完成任务所需的事实。

核心原则：

- 工具 schema 少而稳定；
- 项目速查替代目录和广域文本侦察；
- `read` 指哪读哪；
- `edit` 只传真正变化的字符；
- `run` 返回结构化结论而非终端噪声；
- MCP/CLI 原始输出完整保存在本地；
- 模型投影短、明确、可继续；
- 稳定前缀只追加，不因动态状态反复改写。

## 2. 模型可见工具面

第一阶段常驻核心工具只有五个：

| 工具 | 用途 |
|---|---|
| `project` | 查文件、符号、关系、测试和影响 |
| `read` | 读取精确文件/符号/范围/artifact 片段 |
| `edit` | 基于短文件版本做字符级或最小范围修改 |
| `run` | 运行受控命令、测试和构建 |
| `task` | 用极短状态更新 TaskManager 的步骤、完成或阻塞 |

用户配置 MCP 时，增加一个稳定的 `external` 网关工具，而不是把所有 MCP schema 常驻注入：

| 工具 | 用途 |
|---|---|
| `external` | 查找或调用当前已启用的 MCP 能力 |

不得同时提供同义工具，如 `ls`、`glob`、`grep`、`read_file`、`cat`、`apply_patch`、`shell`、`bash`。必要能力通过上述工具的极少操作提供。`task` schema 保持极短，并允许与首个项目/读取调用并行发出，避免为“写计划”单独增加一轮模型等待。

工具 schema 排序、字段顺序和描述必须稳定。用户未启用 MCP 时，`external` 完全不进入工具列表。

## 3. 统一工具协议

### 3.1 调用前

```text
解析并验证 schema
  -> 规范化项目相对路径/短 ID
  -> 权限判定
  -> 预算与并发判定
  -> 执行
```

别名、缺失字段和 provider replay 噪声在 adapter 层处理，不把复杂兼容逻辑暴露给模型。

### 3.2 结果

统一结果分两层：

```text
RawArtifact       本地完整、可回查、UI 可展开
ModelProjection   发送给主模型的最小结构化事实
```

模型投影的共同字段：

```text
status        ok | error | partial | denied | stale
facts         真正需要的结果
diagnostics   必要错误/警告
changed       修改位置或数量
artifact      可选短句柄，如 A7
omitted       省略数量/字节/行数
next          精确继续参数
```

成功且结果很短时直接返回内容，不强行套冗长 envelope。公共字段只在有信息时出现。

### 3.3 完整性

任何被压缩、分页或省略的结果必须说明：

- 是否完整；
- 原始总量；
- 省略量；
- 短 artifact ID；
- 下一次精确读取方式。

不得把“省略了错误中段”的结果标成完整。

## 4. `project`

接口见 [03-project-intelligence.md](03-project-intelligence.md)。它是侦察入口，默认返回短位置而非源代码。

调用示例：

```text
project { op:"find", q:"parseConfig", limit:5 }
project { op:"links", id:"S42", relation:"callers" }
project { op:"impact", id:"S42", depth:2 }
```

输出示例：

```text
S42 parse_config fn(path)->Config F18:73-101 callers:2 tests:1
```

绝对路径、解析器日志、数据库行和内部身份不得进入模型投影。

## 5. `read`：指哪读哪

### 5.1 输入形态

```text
read { symbol:"S42" }
read { file:"F18", lines:"73-101" }
read { path:"src/config.rs", lines:"73-101" }
read { artifact:"A7", lines:"120-145" }
read { file:"F18", around:"parse_config", context:12 }
```

优先使用 symbol/file 短 ID；明确路径同样支持。目录查询不由 `read` 承担。

### 5.2 输出

```text
F18/v12 src/config.rs 73-101 of 284
73 | fn parse_config(path: &Path) -> Result<Config> {
...
101| }
```

规则：

- 带 1-based 行号；
- 返回短文件版本；
- 默认读取命中符号体或小范围；
- 明确 total、range 和继续位置；
- 不因文件很大自动读取全文件；
- 图片/PDF/二进制采用专用 artifact 预览，不能伪装文本；
- 相同版本相同范围的重复 read 可由运行时直接复用本地结果，不再占模型工具往返。

### 5.3 预算

- 符号读取以实际符号范围为主；
- 范围过大时返回签名、结构和可分页位置；
- 错误、测试失败相关读取优先包含命中行上下文；
- 小结果不压缩；
- 禁止固定粗暴“前 200 行”取代精确范围。

## 6. `edit`：字符级精确修改

### 6.1 设计目标

修改一个字符时，模型只应发送一个字符的旧值和新值，而不是整行、整段或整文件。

调用示例：

```text
edit {
  file:"F18",
  version:12,
  changes:[
    { at:"88:21", old:">", new:">=" }
  ]
}
```

多处修改：

```text
edit {
  file:"F18",
  version:12,
  changes:[
    { at:"88:21", old:">", new:">=" },
    { at:"94:9", old:"pop()", new:"remove(0)" }
  ]
}
```

### 6.2 坐标合同

- `at` 使用 `read` 所显示的 1-based 行与显示列；
- 列的定义在协议中固定为 Unicode scalar position；
- 运行时基于同一 `file/version` 的文本映射到 UTF-8 字节；
- `old` 必须在该位置精确匹配；
- 插入允许 `old:""`；删除允许 `new:""`；
- 需要替换较大块时可用 `range:"88:21-90:4"`，仍只发送范围内旧值；
- 多个 change 必须互不重叠，按原版本坐标解释。

### 6.3 并发与冲突

- 文件版本是短递增整数，不是模型可见哈希；
- 当前版本不同：返回 `stale F18/v13` 和最小重读范围；
- `old` 不匹配：拒绝本次文件修改，不猜位置；
- 单文件所有 change 先验证再原子写入；
- 多文件事务先验证全部文件，再提交；任一失败则一个都不改；
- 写入成功后发布新版本并立即触发图谱增量更新。

### 6.4 输出

成功示例：

```text
ok F18 v12->v13 2 changes +3/-2 chars lines 88,94 tests:not_run
```

默认不把完整 diff 发给模型。完整 diff 作为本地 artifact 给 UI 展开。若模型需要检查，使用 `read` 读取变更范围。

### 6.5 格式化

- 不默认格式化整个文件；
- 只有语言规范要求或用户设置开启时，运行最小范围格式化；
- 格式化产生的额外变化单独列出；
- 若 formatter 无法范围格式化，先预览预计扩大范围，不能悄悄重写全文件；
- 换行风格与文件编码保持不变。

## 7. `run`：安静且结构化

### 7.1 输入

```text
run { task:"test", target:"config" }
run { command:"cargo test -p kf-project" }
```

优先支持结构化 task（test/build/format/lint）和项目检测到的命令；完全访问模式允许任意命令与项目外路径，但永久禁止真实交易、下单、持仓变更和自动购买。

### 7.2 Windows 行为

- 默认隐藏终端窗口；
- stdout/stderr 通过管道捕获；
- PowerShell、CMD 与直接进程分别解析，不用 Bash 规则硬套；
- 取消清理整个子进程树；
- 无固定超时；长任务自然继续，只由用户停止或进程自然结束；
- 默认工作目录为当前项目，绝对路径和 `..` 可访问项目外部；
- 运行中接收用户新引导，在下一次模型请求追加，不重启任务。

### 7.3 模型投影

测试：

```text
error exit:1 tests 118 passed, 2 failed, 1 skipped
fail config::rejects_empty F18:144 expected Err, got Ok
fail merge::keeps_order F31:92 left:[b,a] right:[a,b]
raw:A9 438 lines
```

构建：

```text
error exit:101 3 diagnostics
E0308 F18:88 expected usize, found i32
E0599 F22:41 method not found
raw:A10 912 lines
```

目录：若主模型确需受控目录命令，投影至少保留名称、类型、大小、修改时间和省略数，不能像旧实现只给名字；但正常侦察应使用 `project`。

## 8. `external`：MCP 极简网关

### 8.1 目标

MCP 工具可以很多，常驻全部 schema 会膨胀稳定前缀。KnightFrame 使用单个网关：

```text
external { op:"find", q:"github issue" }
external { op:"call", tool:"E4", args:{...} }
```

### 8.2 本地注册表

完整 MCP schema 只存本地：

- server、tool、说明、参数 schema；
- 权限与风险；
- 中英文别名和关键词；
- 是否只读；
- 最近可用状态；
- 输出 reducer 类型。

`find` 返回少量短工具 ID、用途和必要参数。`call` 时运行时用本地完整 schema 验证，主模型不需要反复读取整份定义。

### 8.3 输出

MCP 原始结果先落 artifact，再按结构投影：

- `structuredContent` 优先；
- 资源链接、图片、音频由 UI 单独展示；
- annotations 用于只读/破坏性提示，但不能代替本地权限；
- 长文本使用内容类型 reducer；
- 失败保持 MCP 原始状态码和必要错误。

## 9. 内容类型压缩

借鉴 Headroom 的“零 LLM、live zone、内容感知”模式，clean-room 实现 Rust reducer：

| 类型 | 默认投影 |
|---|---|
| JSON | 选关键字段、计数、异常项、可继续路径 |
| 测试 | 总数、失败项、位置、断言差异 |
| 编译 | 唯一诊断、位置、错误码、重复折叠 |
| 日志 | 错误/警告窗口、重复计数、时间范围 |
| 搜索 | file:line、符号、短片段、总数 |
| diff | 变更文件、行、最小 hunk、统计 |
| 目录 | 名称、类型、大小、时间、总数 |
| 网络 | 状态、关键 headers、结构化 body 摘要 |
| 网页 | title、正文分块（textChars/omittedChars/nextOffset）、≤32 个交互元素短 ref（e1、e2…含链接 hint）、原始 HTML 不进投影 |

### 9.1 内置浏览器 fetch 与操控

`browser` 工具的 `fetch` 动作在后端抓取页面并做 Playwright MCP 风格的省 token 投影：

- `title` 与状态码直传；
- 正文剥离 script/style 后按 Unicode 字符分块，默认首块 4000 字符，`nextOffset` 精确续读；
- 最多 32 个可交互元素（链接/按钮/输入）压缩为短 ref + 名称 + 链接 hint，超出计数进 `elementsOmitted`；ref 对应的定位信息只保留在本地运行时，后续 `click/fill {ref:"e1"}` 可直接复用，不把 selector 噪声发给模型；
- 原始 HTML 剥离为 `_rawHtml` 后由运行时存为独立本地 artifact（短句柄回执），永不进入模型上下文。

浏览器是主窗口内部的 Tauri 子 WebView，不创建独立顶层窗口。用户与 Agent 共享标签页、地址栏、前进/后退、刷新/停止、页面标题、加载状态和历史状态；网页 `window.open` 被接管为同一主窗口内的新标签页。操控面包含 `open/new-tab/select-tab/close-tab/navigate/back/forward/refresh/stop/close/focus/status`；`click/fill` 优先使用上次 `fetch` 的短 ref，也接受经过 JSON 转义的 CSS selector，`scroll` 按 `y` 像素滚动。定位失效或浏览器未开时返回明确错误 key，不静默失败。

规则：

- 先去 ANSI 和相邻重复；
- 先提取中部唯一错误，再考虑 head/tail；
- 压缩后不更短则使用原结果；
- 小结果直接使用原结果；
- source code、精确 diff、用户要求的原文默认不做语义压缩；
- reducer 异常回退原始短结果或明确分页，不能让工具失败；
- 真实 exit code 不得被简化为布尔值。

## 10. Artifact 回取

不增加独立 `tool_result_retrieve` 工具；统一使用 `read {artifact:"A7", ...}`：

- `lines`：精确行范围；
- `find`：在 artifact 内本地查找并返回位置；
- `around`：命中周围窗口；
- `metadata`：类型、大小、产生工具和完整性。

短句柄只在当前会话有效。artifact 过期时返回明确状态，不能误取其他会话数据。

## 11. 上下文编译

### 11.1 稳定前缀

按顺序固定：

1. 极短核心行为；
2. 当前输出语言与 Caveman Lite 规则；
3. 当前常驻工具 schema；
4. 当前项目固定规则中真正必要的内容。

工具定义必须规范排序。没有动态时间、任务状态、Git 状态、项目进度、记忆、全部 Skill 名单或用量。

### 11.2 动态尾部

按当前 Turn 追加：

1. 被选中的 Skill 正文；
2. 被选中的记忆；
3. TaskManager 当前目标/未完成项；
4. 用户原始请求或可选需求简报；
5. 后续工具投影和主模型消息。

动态内容不得插回或重写稳定前缀。

### 11.3 Prompt 极简规则

- Caveman Lite 用一条稳定短规则实现，不重复列风格示例；
- 工具使用策略只说明 graph-first 与精确 read/edit；
- 权限由运行时执行，不靠 prompt；
- 不在 prompt 描述 UI、指标、功能回执或内部架构；
- 不让模型选择未暴露的工具；
- provider 特有协议由 adapter 处理，避免塞进系统说明。

## 12. 缓存策略

### 12.1 供应商前缀缓存

缓存优化的本质是保持精确前缀稳定：

```text
tools -> system -> append-only messages
```

实现要求：

- system、工具定义和字段顺序确定性序列化；
- 同一会话只向历史尾部追加；
- 权限、进度、时间和动态项目状态放尾部；
- Skill 只在命中 Turn 后追加，不改写稳定基础；
- 模型/工具/schema/输出模式变化记录缓存断裂原因；
- 不为保活固定每几分钟发一次真实模型请求。

只有本地期望收益计算为正时才允许缓存保活，并且用户前台任务到达立即让路。

### 12.2 本地复用

- 相同文件版本与范围的 read 结果复用；
- 相同项目 generation 的 query 结果可短期复用；
- reducer 结果与 artifact 绑定复用；
- 不做默认“语义回答缓存”，避免把旧答案用于变化项目；
- 任何复用都检查短版本/generation，新版本自动失效。

### 12.3 Context compaction

先做确定性处理：

1. 旧工具投影替换为极短结论 + artifact ID；
2. 去除已被后续结果完全覆盖的临时状态；
3. 保留目标、约束、已改文件、失败、验证和下一步；
4. 必要时才进行会话摘要。

`ContextCompactor` 副模型默认关闭，开启时使用独立执行目标。压缩事件替换的是发送给模型的上下文视图，不删除原始本地会话。

不得每轮重写历史来保持短小，因为这会反复破坏前缀缓存。

## 13. 用量与缓存账本

每个 provider response 记录：

- uncached/fresh input；
- cache read；
- cache write；
- output 与 reasoning；
- provider billed cost；
- 若无账单则记录价格来源和估算口径；
- prefix break reason；
- 角色、Turn、调用和模型。

修正参考项目中已发现的缺口：冷请求输入不得漏算，Anthropic cache-write 必须计价，compaction 与副模型调用不能漏记。

## 14. 验收

### 14.1 工具精度

- 修改一个 ASCII 或中文字符时，模型 edit payload 不包含整行/整文件；
- stale version、old mismatch 和重叠 change 全部拒绝且不写文件；
- 多文件验证失败时零文件变化；
- read 可由 symbol 直接定位，无前置目录枚举；
- run 失败保留真实 exit code 和关键中部错误；
- MCP 大 schema 不常驻主模型工具前缀。

### 14.2 压缩完整性

- JSON、测试、构建、日志、diff、目录 golden fixture 的关键字段 100% 保留；
- 每个部分结果有总量、遗漏和继续位置；
- artifact 可按短 ID 精确回取；
- reducer 后 token 不降时自动跳过；
- 错误压缩后绝不变成成功。

### 14.3 缓存

- 相同稳定前缀的重复 Turn 可复现 cache read；
- 动态进度、Task 状态和工具结果不破坏前缀；
- 工具/schema/model 切换有明确 break reason；
- 所有模型调用均进入 usage ledger；
- 账单与 provider usage 在可提供真实账单的适配器上误差目标不超过 1%。
