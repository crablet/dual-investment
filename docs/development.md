# 开发指南

[返回项目首页](../README.md) · [使用指南](usage.md) · [项目约束](../AGENTS.md)

## 环境与运行

- Node.js 24+ 与 pnpm。当前工具链也支持 Node.js 22.13+ 的 22.x 版本。
- Rust 1.95+，与 [Cargo.toml](../src-tauri/Cargo.toml) 的 `rust-version` 一致。
- [Tauri 2 系统依赖](https://v2.tauri.app/start/prerequisites/)。Linux 的凭据存储还需要可用的 Secret Service / keyring 服务。

```bash
git clone https://github.com/crablet/dual-investment.git
cd dual-investment
pnpm install --frozen-lockfile
pnpm tauri:dev
```

`pnpm tauri:dev` 启动桌面客户端。`pnpm dev` 只用于浏览器布局检查：没有 Tauri runtime 时业务命令会报错，不提供模拟交易或浏览器业务替代路径。

TypeScript 暂保留在 6.0.x，以满足 [typescript-eslint 的版本要求](https://typescript-eslint.io/users/dependency-versions/)；升级到 7 前须确认 lint 与 Vue 类型检查工具链兼容。

## 代码入口与边界

修改前阅读 [AGENTS.md](../AGENTS.md)；涉及实盘连接、下单、资金或恢复逻辑时，先阅读[实盘交易安全](trading-safety.md)。

| 目录或入口 | 职责 |
| --- | --- |
| [src/components](../src/components/) | 页面、复核窗口和展示组件 |
| [src/stores](../src/stores/) | Pinia 共享状态、订单进度和语言偏好 |
| [src/services/api.ts](../src/services/api.ts) | Tauri IPC 调用与错误契约校验 |
| [src/i18n.ts](../src/i18n.ts)、[src/locales](../src/locales/) | 消息翻译与语言词典加载 |
| [src-tauri/src/app_state](../src-tauri/src/app_state/) | 连接、交易、恢复、结算与后台维护编排 |
| [src-tauri/src/strategy](../src-tauri/src/strategy/) | Decimal 金融计算与策略规则 |
| [src-tauri/src/persistence.rs](../src-tauri/src/persistence.rs) | SQLite 订单和恢复记录 |
| [src-tauri/src/secrets.rs](../src-tauri/src/secrets.rs) | 系统 keyring 凭据读写 |

- [AppState](../src-tauri/src/app_state.rs) 编排交易、恢复和后台维护。`get_app_snapshot` 只读取本地状态和账户缓存；`sync_live_state` 完成同步后不返回快照，由 Pinia 再读取完整状态。
- [策略层](../src-tauri/src/strategy.rs) 使用 Decimal 计算金额、数量和收益，前端以字符串接收。`preview_strategy` 只做本地预估；金融字段在前端的数值转换仅用于格式化和图形比例。
- [错误分类](../src-tauri/src/errors.rs) 决定查询重试、提交结果是否待确认和连接重试资格。[IPC](../src-tauri/src/commands.rs) 的业务错误返回 `{ message: { code, params }, canAutoReconnect }`，前端不根据文案判断重连；Tauri 自身的传输或参数错误不授予自动重连资格。
- [消息结构](../src-tauri/src/messages.rs) 在 IPC、SQLite 和日志中保存 `{ code, params }`；参数为字符串或嵌套消息，金融参数保留 Decimal 字符串。前端通过 `messageText()` 在展示时递归翻译；Rust 的 `Display` 使用简体中文词典，仅供进程诊断。
- [本地日志](../src-tauri/src/logs.rs) 按本机日期写入 `logs/YYYY-MM-DD.log`，每行一条 JSON。日志面板按当前语言翻译消息；日志级别与事件代码保持原值，切换语言不改写日志文件。

开发期不保留旧 IPC、DTO 或 SQLite 兼容与迁移分支。当前结构化消息格式不兼容旧的纯文本消息记录；应用不会自动迁移或删除已有数据库与日志。处理已有开发数据前，应先明确对应的订单和恢复状态。

## 多语言维护

- [翻译入口](../src/i18n.ts) 使用 Vue I18n，英语词典随主包提供，其余词典通过动态导入独立分包；保留按需加载，不改回一次性载入全部语言。已加载词典在当前会话复用，同一语言的并发加载共用请求。
- [语言偏好](../src/stores/preferences.ts) 由 Pinia 管理，保存在 `localStorage` 的 `dual-investment.locale` 键中。[应用入口](../src/main.ts) 等待语言初始化后再挂载界面；切换成功后才更新当前语言和保存偏好，过期请求不能覆盖后续选择。存储不可用时仍允许本次会话切换。
- 文案位于 [src/locales](../src/locales/)，新增或删除消息时同步全部语言的消息键与插值参数。业务消息保留结构，展示时使用 `t()` 或 `messageText()`；合约代码、资产符号、状态值和日志事件代码保持稳定，翻译对应的展示标签。
- 新增语言时同时更新 `src/i18n.ts` 的 `supportedLocales`、[语言选择器](../src/components/settings/GeneralSettingsPanel.vue) 的原生语言名称，以及 [App.vue](../src/App.vue) 中 Naive UI 的界面与日期语言映射；需要本地化的数字和日期展示通过 `formatLocale()` 选择格式，保持系统本地时区和公历约定。

## 验证与打包

按改动范围执行检查：

| 改动范围 | 检查 |
| --- | --- |
| 前端 | `pnpm lint`、`pnpm typecheck` |
| 依赖、构建、工具配置、词典或语言加载 | 另运行 `pnpm build` |
| Rust | `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`，再运行 `cargo fmt --manifest-path src-tauri/Cargo.toml` |
| UI | 检查 1180px 最小宽度和较宽窗口，包含较长的多语言文案；设置页连接与通用设置保持并排，按钮文案不能被截断 |
| Tauri 启动、图标或窗口 | 实际启动客户端验证 |

词典变更须核对全部语言的消息键与插值参数。语言加载变更须核对首次系统语言、手动选择与重启记忆、快速切换、加载失败回退和缓存复用。构建产物应保留独立语言分包，启动仅加载英语回退和当前语言。

遵循 [AGENTS.md](../AGENTS.md)：验证使用的临时测试、夹具、快照及其依赖和配置不保留在交付中。

### Linux 安装包

```bash
pnpm tauri:build
```

当前 [Tauri 配置](../src-tauri/tauri.conf.json) 生成 `.deb` 和 `.rpm`，默认产物目录为 `src-tauri/target/release/bundle/`。

只生成可执行文件：

```bash
pnpm tauri:build --no-bundle
```

默认路径为 `src-tauri/target/release/dual-investment`。

## 文档与截图

面向使用者的介绍维护在 [README.md](../README.md) 和 [README.en.md](../README.en.md)，操作细节与计算口径维护在[使用指南](usage.md)，内部实现与检查流程维护在本页。

README 产品图片保存在 `docs/images/`，使用可公开的实际界面截图，不注入或改写行情、订单与收益数据。更新图片时同步检查中英文 README 的说明；浏览器验证生成的临时快照和其他调试产物仍须清理。

## 常见环境问题

- **浏览器提示命令只能在 Tauri 客户端中执行**：使用 `pnpm tauri:dev` 启动桌面客户端；浏览器预览只用于 UI 检查。
- **无法访问系统凭据存储**：检查桌面会话的 Secret Service / keyring 服务是否可用。应用不会降级为明文文件保存凭据。
- **语言加载失败**：界面保留当前语言，启动阶段失败时使用英语；重启应用后重试。构建时应一并保留全部语言资源文件。
