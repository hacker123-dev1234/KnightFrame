# KnightFrame

> Beta · Windows 本地编程 Agent

[English](README.en.md)

KnightFrame 解决三类常见浪费：反复遍历项目、把完整工具输出塞回上下文、会话前缀频繁变化导致缓存失效。

## 怎么做

- **项目索引**：记录文件、符号、行号和引用关系。先查询索引，再读取需要的行。
- **精确工具**：完整结果留在本地，只把必要片段交给模型；读、写、搜索和运行结果可复用。
- **稳定上下文**：系统提示、工具定义和历史按固定顺序追加，减少重复输入。
- **用量可见**：显示输入、输出、缓存命中和估算费用。

模型、小模型、Skill 和记忆均由用户配置。记忆默认关闭。

## 单次对照

同一模型、10 个修复任务、每项只运行一次：

| 指标 | KnightFrame | 对照组 |
| --- | ---: | ---: |
| 完成 | 9 / 10 | 8 / 10 |
| 总 Token | 426,941 | 1,610,622 |
| 请求次数 | 65 | 101 |
| 用时 | 25:15 | 31:19 |

这是一次测量，不代表稳定排名。原始汇总和计分方式见 [测试记录](docs/benchmark-2026-08-18.md)。

## 当前状态

支持项目索引、对话、模型适配、精确读写、命令运行、网页搜索、内置浏览器和插件工坊。当前仅重点测试 Windows，仍可能遇到供应商协议变化和长任务中断。

## 构建

需要 Rust stable、Node.js 20+、pnpm 9+、WebView2 和 Visual Studio C++ Build Tools。

```powershell
pnpm install
pnpm check
pnpm test
pnpm build:test-exe
```

发布构建：

```powershell
pnpm build:release
```

## 插件工坊

KnightFrame 预览已内嵌，不需要另一份源码。外部宿主预览为可选功能：构建宿主源码后，将 `KF_DSH_ROOT` 指向包含 `apps/cli/lib/bin.js` 的根目录，再重启 KnightFrame。

```powershell
[Environment]::SetEnvironmentVariable("KF_DSH_ROOT", "D:\Projects\host", "User")
```

Apache-2.0。项目使用 OpenAI Codex 协助开发与检查。
