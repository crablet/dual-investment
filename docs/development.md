# 开发指南

[返回项目首页](../README.md) · [项目约束](../AGENTS.md) · [实盘交易安全](trading-safety.md)

## 环境与运行

使用 Node.js 24+、pnpm 12.9.1、Rust 1.95+。包管理器和最低 Rust 版本分别定义在 [package.json](../package.json) 与 [Cargo.toml](../src-tauri/Cargo.toml)，CI 工具链以[工作流](../.github/workflows/build-desktop.yml)为准。

安装下述平台依赖后，按 [README 快速开始](../README.md#快速开始)运行。`pnpm dev` 仅用于浏览器界面预览，无 Tauri runtime 时业务命令报错。

### 平台环境

| 平台 | 构建与运行依赖 | 凭据存储 |
| --- | --- | --- |
| Linux x64 | C/C++ 工具链、`pkg-config`、WebKitGTK 4.1、GTK、D-Bus 开发库；见 [Tauri Linux 依赖](https://v2.tauri.app/start/prerequisites/#linux)。桌面会话须有 Secret Service。 | 系统 keyring |
| Windows 10 / 11 x64 | [Microsoft C++ Build Tools](https://v2.tauri.app/start/prerequisites/#windows)、Windows SDK、Rust MSVC 工具链、WebView2、Git。使用原生 Windows 终端；WSL 生成 Linux 应用。 | Windows 凭据管理器 |
| macOS 13.3+ | Xcode Command Line Tools（`xcode-select --install`）、Rust、Git。支持 Apple Silicon 与 Intel。 | macOS Keychain |

Windows 安装包未签名；macOS 为 ad-hoc 签名、未公证，首次打开可能需按 [Apple 说明](https://support.apple.com/en-us/102445)允许运行。平台验收状态见 [README](../README.md#安装与平台状态)；Ubuntu 26.04 的 CI 构建不代表兼容所有 Linux 发行版。

## 代码结构

| 入口 | 职责 |
| --- | --- |
| [src/components](../src/components/)、[src/stores](../src/stores/) | 页面与 Pinia 共享状态 |
| [src/services/api.ts](../src/services/api.ts)、[commands.rs](../src-tauri/src/commands.rs) | IPC 与错误契约 |
| [app_state.rs](../src-tauri/src/app_state.rs)、[app_state/](../src-tauri/src/app_state/) | 连接、交易、恢复与后台维护 |
| [positions.rs](../src-tauri/src/app_state/positions.rs)、[settlement.rs](../src-tauri/src/app_state/settlement.rs) | 净仓同步与交割流程 |
| [strategy/](../src-tauri/src/strategy/) | Decimal 金融计算与策略规则 |
| [persistence.rs](../src-tauri/src/persistence.rs) | SQLite 持久化与账户绑定 |
| [secrets.rs](../src-tauri/src/secrets.rs) | 系统凭据存储 |
| [i18n.ts](../src/i18n.ts)、[locales/](../src/locales/) | 文案与语言加载 |

### 状态与消息

前端读取 `get_app_snapshot`；手动同步调用 `sync_live_state` 后重新取快照。后台每分钟尝试维护交易与交割，页面切换不新增交易所轮询。并发、资金预留和恢复规则集中在[实盘交易安全](trading-safety.md)。

| 模型 | 数量和状态的含义 |
| --- | --- |
| `StrategyOrder` | 本机委托历史及实际成交；委托终态不代表平仓或交割完成，取消不抹去部分成交 |
| `TrackedPosition` | 以合约为键的当前卖方净仓，驱动持仓展示与到期登记 |
| `DeliveryTask` | 同合约唯一的交割义务，独立保存行权事实和现货订单引用；任务完成与批次资金收尾分开 |

完整字段与状态见 [Rust 领域模型](../src-tauri/src/domain.rs)和 [TypeScript 类型](../src/types/domain.ts)。净仓、历史成交量和交割量不能互相替代；未知行权数量与真实零值也不能混用。

业务消息通过 [Message](../src-tauri/src/messages.rs) 的 `{ code, params }` 保存，在前端用 `messageText()` 翻译，金融参数保持 Decimal 字符串。日志沿用结构化消息，级别与事件代码不翻译。

业务错误返回 `{ message, canAutoReconnect }`。[errors.rs](../src-tauri/src/errors.rs)判断是否允许自动重连，前端校验该字段后使用。Tauri 通信、参数或格式错误不会触发自动重连。

## 多语言维护

1. 修改 [src/locales](../src/locales/) 时同步全部语言的消息键与插值参数；资产符号、合约和状态代码保持稳定，仅翻译展示标签。
2. 新增语言时更新 `src/i18n.ts` 的 `supportedLocales`、[语言选择器](../src/components/settings/GeneralSettingsPanel.vue)的原生名称及 [App.vue](../src/App.vue) 的 Naive UI / 日期映射。
3. 数字和日期经 `formatLocale()` 格式化，保持系统时区与公历。部分 WebView 不支持世界语 `Intl`，沿用法语数字格式与[日历工具](../src/utils/date.ts)中的世界语词典。

英语随主包加载，其余词典按需加载。加载成功后才保存语言选择；旧请求不能覆盖新选择，失败时保留当前语言。切换语言不触发连接或交易。新增语言要在浏览器检查，Node.js 支持的语言不一定受 WebView 支持。

## 验证与打包

| 改动范围 | 检查 |
| --- | --- |
| 文档 | 链接、文档间一致性、命令与当前配置 |
| 前端 | `pnpm lint`、`pnpm typecheck` |
| 依赖、构建配置、词典或语言加载 | 另运行 `pnpm build`，确认语言仍独立分包 |
| Rust | `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`，再运行 `cargo fmt --manifest-path src-tauri/Cargo.toml` |
| UI | 检查 1180px 最小宽度、宽窗口和较长翻译；按钮文字不能截断 |
| Tauri 启动、图标或窗口 | 隔离数据与凭据后启动验证，不连接真实账户 |

交易验证仅用隔离存储与交易所 / IPC 替身，不读取用户真实凭据或数据库，不发送实盘下单、划转或申赎。净仓与交割改动须覆盖重复下单、到期前平仓、离线跨期、首次发现已到期，以及查询失败和未知数量。临时验证产物按 [AGENTS.md](../AGENTS.md)清理。

### 保持既有界面

内部实现优化须保留原有布局、字号、间距、颜色、截断规则、悬停提示、图表与弹窗效果。无障碍名称和状态可通过不影响布局的属性补充；如果改写无法保持显示一致，保留现有组件实现。界面验证使用相同数据、窗口尺寸和语言，与改动前逐页对照。

### 桌面安装包

```bash
pnpm tauri:build
```

默认产物在 `src-tauri/target/release/bundle/`，公共配置与当前平台配置自动合并：

| 平台 | 配置 | 产物 |
| --- | --- | --- |
| Linux | [tauri.linux.conf.json](../src-tauri/tauri.linux.conf.json) | `.deb`、`.rpm` |
| Windows | [tauri.windows.conf.json](../src-tauri/tauri.windows.conf.json) | NSIS `-setup.exe` |
| macOS | [tauri.macos.conf.json](../src-tauri/tauri.macos.conf.json) | `.app`、`.dmg` |

本地默认构建当前架构。在 macOS 构建通用版：

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
pnpm tauri:build --target universal-apple-darwin -- --locked
```

指定 `--target` 的产物位于 `src-tauri/target/<target>/release/bundle/`。只需可执行文件时使用 `pnpm tauri:build --no-bundle`。

### 自动构建与发布

[工作流](../.github/workflows/build-desktop.yml)将 Linux、Windows 和 macOS 安装包上传到 Releases 草稿：

1. 同步 `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 和 `src-tauri/Cargo.lock` 的项目版本，完成上述检查。
2. 提交代码并推送匹配的版本标签，如 `v0.1.0`；或在 **Actions → Build desktop installers → Run workflow** 指定已有标签。
3. 等三个平台任务成功并验收安装、启动和凭据存储后，更新平台状态和发布说明，再手动发布草稿。重跑复用未发布草稿，已发布版本须用新标签。

所有 job 在分配 runner 前跳过非公开仓库，仅使用标准 runner 和 `GITHUB_TOKEN`；不启用缓存、Actions artifacts 或付费签名服务，CI 不启动客户端。修改这些限制前核对 [Actions 费用](https://docs.github.com/en/billing/concepts/product-billing/github-actions)。

## 文档与截图

README 介绍功能和操作，本页说明开发和发布，交易规则见[安全文档](trading-safety.md)。其他位置用链接引用，避免重复维护。

更新中英文 README 时同步图文；`docs/images/` 中的产品截图使用可公开的真实界面，不注入或改写行情、订单与收益数据。

## 常见环境问题

- **浏览器业务命令报错**：通过 `pnpm tauri:dev` 使用桌面客户端。
- **凭据存储不可用**：检查系统 keyring；Linux 须有可用的 Secret Service。不会回退到明文凭据文件。
- **语言加载失败**：保留当前语言，启动阶段回退英语；确认构建产物包含全部语言资源后重试。
