# KnightFrame Rust 开发文档

## 1. 文档用途

本组文档是 KnightFrame Rust 重构的开发合同，不是愿望清单。它同时规定：

1. 产品做什么、不做什么；
2. 主模型、副模型、运行时、工具、图谱与 UI 的责任边界；
3. 每项功能如何触发、用户如何知道它确实执行、如何度量、如何验收；
4. 如何证明 KnightFrame 在同任务、同模型、同质量口径下比 Claude Code 至少节省 20%；
5. 哪些参考项目只能借鉴模式，哪些内容因许可证、证据质量或架构目标不能复制。

旧 Kotlin KnightFrame、旧 LexSilva、反编译项目和第三方项目均不得成为新工程的隐式依赖。

## 2. 阅读顺序

| 顺序 | 文档 | 回答的问题 |
|---:|---|---|
| 1 | [00-product-contract.md](00-product-contract.md) | 什么是成功，哪些原则不可妥协 |
| 2 | [01-system-architecture.md](01-system-architecture.md) | Rust/Tauri 系统怎样分层，事件怎样流动 |
| 3 | [02-model-roles.md](02-model-roles.md) | 主模型和副模型怎样物理隔离，何时允许调用副模型 |
| 4 | [03-project-intelligence.md](03-project-intelligence.md) | 全量项目速查如何取代反复目录、全文读取和 grep |
| 5 | [04-tools-context-cache.md](04-tools-context-cache.md) | 工具如何极简、精确，缓存如何保持稳定 |
| 6 | [05-task-skills-memory.md](05-task-skills-memory.md) | TaskManager、SkillOpt、九层记忆、GrillMe 如何工作 |
| 7 | [06-ui-i18n.md](06-ui-i18n.md) | 最终桌面体验、黑白视觉、国际化如何落地 |
| 8 | [07-providers-security.md](07-providers-security.md) | API 适配、开源模型、Windows 权限和隐私如何实现 |
| 9 | [08-delivery-verification.md](08-delivery-verification.md) | 分期、自动化测试、成本基准和发布门禁是什么 |
| 10 | [09-requirements-traceability.md](09-requirements-traceability.md) | 每项用户要求落在哪个组件和验收项 |
| 11 | [10-feature-contracts.md](10-feature-contracts.md) | 每项功能的触发、回执、指标和成品验收是什么 |
| 12 | [11-harness-parity-roadmap.md](11-harness-parity-roadmap.md) | 如何完整对齐主流 coding harness 且保持极简传输 |
| 13 | [12-plugins-studio.md](12-plugins-studio.md) | DSH/Cordis 插件兼容、跨语言 runner 与可视化 DIY 如何工作 |
| 14 | [REFERENCES.md](REFERENCES.md) | 参考来源、许可证和证据边界是什么 |

## 3. 规范用词

下列词语具有约束意义：

- **必须**：不满足就不能合并或发布。
- **不得**：明确禁止。
- **应**：默认实现方式；偏离需要在架构决策记录中说明原因。
- **可选**：用户主动开启或特定能力存在时才启用；不得暗中产生调用或费用。
- **主模型**：唯一负责用户任务推理、工具调用和最终回答的模型。
- **副模型**：用户按功能显式启用后，承担单一辅助工作的独立模型部署。
- **本地确定性逻辑**：不调用任何模型、相同输入产生相同结果的 Rust 逻辑。
- **项目速查**：本地持久化的全量文件、符号、关系、配置与测试索引；不是单纯可视化图。
- **模型投影**：工具完整结果中真正发送给模型的最小结构化部分。
- **原始结果**：本地保存的完整工具结果，默认不进入模型上下文。
- **功能回执**：用户可见、可回查的“本次是否执行、为什么、效果和成本”记录。

## 4. 变更流程

任何新增功能必须先完成以下文档内容：

1. 功能 ID 与负责人模块；
2. 默认开关状态；
3. 精确触发条件；
4. 当轮可见回执；
5. 效果、成本、延迟和失败指标；
6. Windows 打包产物上的验收；
7. 中英文资源键；
8. 对主模型 prompt、工具 schema 和缓存前缀的 token 影响。

缺少任意一项时，功能状态只能是 `designed`，不得在正式 UI 中显示为可用。

## 5. 功能就绪状态

```text
proposed -> designed -> implemented -> verified -> shipped
```

- `proposed`：只有想法。
- `designed`：触发、回执、指标、验收和边界已写清。
- `implemented`：代码路径存在，尚未完成成品验收。
- `verified`：自动化测试与 Windows 成品测试通过。
- `shipped`：已进入正式签名安装包。

`enabled=true` 不等于执行过，组件存在不等于功能完成，UI 有入口也不等于模型真正受益。
