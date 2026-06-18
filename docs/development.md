# 开发指南

[返回项目首页](../README.md) · [使用指南](usage.md) · [项目约束](../AGENTS.md)

## 环境与运行

- Node.js 24+ 与 pnpm 12.9.1，pnpm 版本固定在 [package.json](../package.json)。当前工具链也支持 Node.js 22.13+ 的 22.x 版本。
- Rust 1.95+，与 [Cargo.toml](../src-tauri/Cargo.toml) 的 `rust-version` 一致；发布流水线固定使用 Rust 1.99.1。
- [Tauri 2 系统依赖](https://v2.tauri.app/start/prerequisites/)。Linux 的凭据存储还需要可用的 Secret Service / keyring 服务。

```bash
git clone https://github.com/crablet/dual-investment.git
cd dual-investment
pnpm install --frozen-lockfile
pnpm tauri:dev
```

`pnpm tauri:dev` 启动桌面客户端。`pnpm dev` 只用于浏览器布局检查：没有 Tauri runtime 时业务命令会报错，不提供模拟交易或浏览器业务替代路径。

TypeScript 暂保留在 6.0.x，以满足 [typescript-eslint 的版本要求](https://typescript-eslint.io/users/dependency-versions/)；升级到 7 前须确认 lint 与 Vue 类型检查工具链兼容。

### 平台环境

三个桌面平台共享业务代码，但当前仅有 Linux 的实际使用记录。Windows 与 macOS 已配置原生凭据存储、图标和安装包目标，仍须通过首次 CI 构建及实机验证；构建成功不代表安装、Keychain / 凭据管理器或实盘流程已经验收。

| 平台 | 从源码构建所需环境 | 凭据保存位置 |
| --- | --- | --- |
| Linux x64 | C/C++ 编译工具、`pkg-config`、WebKitGTK 4.1、GTK、D-Bus 开发库；按 [Tauri Linux 依赖](https://v2.tauri.app/start/prerequisites/#linux)安装，并确保桌面会话有可用的 Secret Service。 | Secret Service / 系统 keyring |
| Windows 10 / 11 x64 | [Microsoft C++ Build Tools](https://v2.tauri.app/start/prerequisites/#windows) 的“使用 C++ 的桌面开发”工作负载及 Windows SDK、Rust MSVC 工具链、WebView2 Evergreen Runtime、Git。 | Windows 凭据管理器 |
| macOS 13.3+，Apple Silicon / Intel | Xcode Command Line Tools（`xcode-select --install`）、Rust、Git。 | macOS Keychain |

安装 Node.js 后可用 `npm install --global pnpm@12.9.1` 安装项目使用的 pnpm。Windows 请在原生 PowerShell 或其他 Windows 终端执行上方启动命令；WSL 构建的是 Linux 应用。

macOS 打包下限设为 13.3，以配合当前 `es2022` 前端构建目标及对应的 [Safari 16.4 / WebKit 能力](https://webkit.org/blog/13966/webkit-features-in-safari-16-4/)。这是配置的支持下限，实机兼容性仍待验证。

安装包的签名状态与首次运行：

- Windows：运行 `-setup.exe`，缺少 WebView2 时安装器会下载运行时。当前未购买发布者签名证书，可能显示 SmartScreen 提示。
- macOS：打开 `.dmg`，拖入“应用程序”。采用[免费 ad-hoc 签名](https://v2.tauri.app/distribute/sign/macos/#ad-hoc-signing)，未做 Apple 公证；确认来源后可按 [Apple 官方说明](https://support.apple.com/en-us/102445)在“隐私与安全性”中允许该应用。
- Linux：通过包管理器安装对应格式，运行时需要发行版提供的 WebKitGTK 4.1 与兼容系统库。CI 使用 Ubuntu 24.04 构建，不能据此推断所有 Linux 发行版均已兼容。

## 代码入口与边界

修改前阅读 [AGENTS.md](../AGENTS.md)；涉及实盘连接、下单、资金或恢复逻辑时，先阅读[实盘交易安全](trading-safety.md)。

| 目录或入口 | 职责 |
| --- | --- |
| [src/components](../src/components/) | 页面、复核窗口和展示组件 |
| [src/stores](../src/stores/) | Pinia 订单历史、净仓、交割快照、请求进度和语言偏好 |
| [src/services/api.ts](../src/services/api.ts) | Tauri IPC 调用与错误契约校验 |
| [src/i18n.ts](../src/i18n.ts)、[src/locales](../src/locales/) | 消息翻译与语言词典加载 |
| [src-tauri/src/app_state](../src-tauri/src/app_state/) | 连接、交易、恢复、结算与后台维护编排 |
| [src-tauri/src/app_state/positions.rs](../src-tauri/src/app_state/positions.rs) | 卖方净仓同步、到期前登记与交割任务生成 |
| [src-tauri/src/strategy](../src-tauri/src/strategy/) | Decimal 金融计算与策略规则 |
| [src-tauri/src/persistence.rs](../src-tauri/src/persistence.rs) | SQLite 账户归属、订单历史、净仓、交割任务与恢复记录 |
| [src-tauri/src/secrets.rs](../src-tauri/src/secrets.rs) | 系统 keyring 凭据读写 |

- [AppState](../src-tauri/src/app_state.rs) 编排交易、恢复和后台维护。`get_app_snapshot` 只读取本地状态和账户缓存；`sync_live_state`、`recover_order_attempt` 完成命令后由 Pinia 再读取完整快照。界面不提供手动交割扫描或手工补齐标记入口。
- 手动连接与保存凭据连接共用账户归属校验：从 `GET /api/v3/account` 解析 UID，经权限验证后写入或核对 SQLite 单行 `account_binding`，再更新凭据与运行态。UID 只在 Rust 和本地数据库中使用；归属不随删除凭据改变，不增加逐订单账户字段或额外查询。
- [策略层](../src-tauri/src/strategy.rs) 使用 Decimal 计算金额、数量和收益，前端以字符串接收。`preview_strategy` 只做本地预估；金融字段在前端的数值转换仅用于格式化和图形比例。
- [错误分类](../src-tauri/src/errors.rs) 决定查询重试、提交结果是否待确认和连接重试资格。[IPC](../src-tauri/src/commands.rs) 的业务错误返回 `{ message: { code, params }, canAutoReconnect }`，前端不根据文案判断重连；Tauri 自身的传输或参数错误不授予自动重连资格。
- [消息结构](../src-tauri/src/messages.rs) 在 IPC、SQLite 和日志中保存 `{ code, params }`；参数为字符串或嵌套消息，金融参数保留 Decimal 字符串。前端通过 `messageText()` 在展示时递归翻译；Rust 的 `Display` 使用简体中文词典，仅供进程诊断。
- [本地日志](../src-tauri/src/logs.rs) 按本机日期写入 `logs/YYYY-MM-DD.log`，每行一条 JSON。日志面板按当前语言翻译消息；日志级别与事件代码保持原值，切换语言不改写日志文件。

### 刷新与后台维护

- [前端调度器](../src/composables/useBackgroundSync.ts) 启动后先读取快照并尝试使用已保存凭据连接，5 秒后开始定时检查；正常每 60 秒读取一次本地快照，读取失败时退避至最多 300 秒，连接重试使用单独的退避规则。页面切换不启动额外的交易所轮询。
- Rust 维护任务每分钟尝试恢复资金请求、查询待确认现货原单、同步未终结的期权委托和当前净仓，再发现并推进交割批次。维护锁繁忙时跳过；账户展示刷新在维护锁释放后单独执行。
- `sync_live_state` 读取交易所委托、净仓和账户状态，并登记已有到期义务的批次，返回 `()`；它不推进现货下单或资金动作。Pinia 随后调用 `get_app_snapshot` 更新展示。

### 订单、净仓与交割任务

这三类事实分别持久化，不能互相充当数量来源：

| 模型 / 表 | 事实与身份 | 前端用途 |
| --- | --- | --- |
| `StrategyOrder` / `strategy_orders` | 本机期权订单历史，ID 为 `OPTION-{完整合约}-{交易所订单号}`；恢复查询复用同一记录，保存成交数量、均价与合约 `unit` | 本机订单历史、已记录卖出权利金；不累计为交割义务 |
| `TrackedPosition` / `option_positions` | 当前账户的合约卖方净仓，以完整 `option_symbol` 为主键；保留 `trackedAt`、`lastSyncedAt` 和 `open/closed/expired` | 总览、持仓、候选持仓标识、到期日历 |
| `DeliveryTask` / `delivery_tasks` | 仅由到期前已登记且未停止跟踪的仓位创建；同合约唯一，独立保存行权事实和现货订单引用 | 交割任务与进度；当前净仓消失不会删除任务 |

`StrategyOrder.status` 仅表示委托状态：`pending`、`partiallyFilled` 继续同步，`filled`、`cancelled`、`rejected`、`expired` 为终态。委托终结不代表净仓已平仓或交割已完成，部分成交数量也不因委托取消而清零。

连接、手动同步和后台维护读取 Binance 当前 BTC/USDT、USDT 报价的卖方净仓。客户端解析保留持仓数量的正负号，只跟踪负数量，入库时保存其绝对张数；买方与其他合约不进入跟踪。到期前成功查询确认仓位消失或不再是卖方时置为 `closed`；查询失败保留已有数量，`positionSync.error` 返回错误，`positionSync.updatedAt` 保留本次连接内最后成功同步时间。每条持仓另以 `lastSyncedAt` 持久化其更新时间。界面不得把错误响应当成空列表替换仓位。

到期任务仅来源于本机已有登记。进程离线跨过到期时，原 `open` 记录可冻结为 `expired` 并生成待确认任务；这一步先于本轮交易所查询，查询失败也不会抹去已登记义务。首次登记资格使用持仓和合约查询全部完成后的时间判断；响应跨过到期时刻不能登记新义务。到期后首次出现的仓位、历史成交或历史行权不能新增义务。净仓更新与首次交割任务在同一事务中提交，重复扫描不重置已有任务。结算批次的 `taskIds` 引用交割任务 ID。

`DeliveryTask` 使用独立的交割状态；已有现货客户端订单号时只查询原单：

| 状态 | 含义 |
| --- | --- |
| `waitingExercise` | 等待账户行权事实或公开结算价确认无需交割 |
| `ready` | 价内交割数量与结算价已确认，等待备资及发送现货请求 |
| `executing` | 已登记现货请求，等待确认原单成交结果 |
| `completed` | 现货足额成交，或确认无需交割 |
| `failed` | 现货提交明确失败或成交异常 |

账户行权记录按合约、SHORT、Put/Call 与到期时间筛选，按记录 ID 去重后汇总绝对张数；原始数量符号参与同 ID 冲突检查，现金金额保留交易所正负号。查询满页时拆分时间窗，仍无法确认完整性则等待后续查询。实现见 [Binance 客户端](../src-tauri/src/binance.rs) 与 [交割编排](../src-tauri/src/app_state/settlement.rs)。

`DeliveryTask.quantity`、`settlementPrice`、`cashSettlement` 初始为 `null`。公开历史按同标的、同到期时刻读取 `realStrikePrice`；确认未价内时可以直接完成任务，此时 `settlementPrice` 有值，而没有账户记录的 `quantity`、`cashSettlement` 仍为 `null`。`lastPositionQuantity` 仅作核对，不是发送现货订单的后备数量。所有金额和数量由 Rust `Decimal` 计算，单个合约的张数与 `unit` 分开传输；组合 BTC 数量由 Rust 计算，前端不自行相乘。

交割任务完成与批次资金收尾是两个状态。整批任务完成后进入 `pendingSweep`，至少等待 5 分钟再归集；批次到达 `completed` 或符合既有人工收尾条件的 `sweepSkipped` 才解除该批次的资金预留。

`PnlReport` 仅按本机成交订单汇总 `recordedPremium`、`todayRecordedPremium`、`sellPutPremium` 和 `sellCallPremium`，供收益分析页展示权利金总额、今日权利金、方向分布与成交记录。不汇总行权现金流、期权或现货浮动盈亏，不从净仓成本倒推历史权利金，也不发布全账户累计收益。对应计算在[资产与收益派生](../src-tauri/src/strategy/balances.rs)。

专用账户与禁止外部手工补齐属于使用约定，详见[使用指南](usage.md#自动交割与恢复)。实现不增加“猜测用户已在其他终端补齐”的检测或人工标记分支；不能用外部现货余额变化替代本任务的订单确认事实。

开发期不保留旧 IPC、DTO 或 SQLite 兼容与迁移分支。旧的“策略订单同时承担持仓和交割”结构不受支持；切换此开发版本时应关闭应用，明确处理原义务后使用空数据库。清空数据库会失去旧登记记录，之后不能通过历史行权自动重建交割资格。应用代码不自动迁移或删除已有数据库与日志，清理 SQLite 不应调用删除系统 keyring 凭据的命令。

## 多语言维护

- [翻译入口](../src/i18n.ts) 使用 Vue I18n，英语词典随主包提供，其余词典通过动态导入独立分包；保留按需加载，不改回一次性载入全部语言。已加载词典在当前会话复用，同一语言的并发加载共用请求。
- [语言偏好](../src/stores/preferences.ts) 由 Pinia 管理，保存在 `localStorage` 的 `dual-investment.locale` 键中。[应用入口](../src/main.ts) 等待语言初始化后再挂载界面；切换成功后才更新当前语言和保存偏好，过期请求不能覆盖后续选择。存储不可用时仍允许本次会话切换。
- 文案位于 [src/locales](../src/locales/)，新增或删除消息时同步全部语言的消息键与插值参数。业务消息保留结构，展示时使用 `t()` 或 `messageText()`；合约代码、资产符号、状态值和日志事件代码保持稳定，翻译对应的展示标签。
- 语言选择器将“跟随系统”置顶，其余原生语言名称按固定 `en` 排序规则比较，切换界面语言不改变选项顺序。
- 新增语言时同时更新 `src/i18n.ts` 的 `supportedLocales`、[语言选择器](../src/components/settings/GeneralSettingsPanel.vue) 的原生语言名称，以及 [App.vue](../src/App.vue) 中 Naive UI 的界面与日期语言映射；需要本地化的数字和日期展示通过 `formatLocale()` 选择格式，保持系统本地时区和公历约定。
- 部分 WebView 的 `Intl` 不支持世界语。世界语的数字与 24 小时时间使用相同惯例的法语格式；[日历日期工具](../src/utils/date.ts) 直接复用 Naive UI 的世界语月份、星期词典，避免回退到设备默认语言。新增语言须在浏览器中检查日期与数字格式，不能仅依赖 Node.js 的 ICU 支持。

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

净仓与交割变更的临时验证应覆盖同合约多笔订单不重复计仓、到期前平仓停止跟踪、到期后首次出现不追溯、已登记仓位离线跨期可恢复、同步失败保留事实，以及未知交割数量和真实零值的区别。验证必须使用隔离存储与交易所/IPC 替身；不读取用户真实凭据或数据库，不通过真实下单、划转或申赎验证代码。

遵循 [AGENTS.md](../AGENTS.md)：验证使用的临时测试、夹具、快照及其依赖和配置不保留在交付中。

### 桌面安装包

在对应系统执行同一条命令：

```bash
pnpm tauri:build
```

Tauri 将[公共配置](../src-tauri/tauri.conf.json)与平台文件自动合并，默认产物目录为 `src-tauri/target/release/bundle/`：

| 当前系统 | 平台配置 | 产物 |
| --- | --- | --- |
| Linux | [tauri.linux.conf.json](../src-tauri/tauri.linux.conf.json) | `deb/*.deb`、`rpm/*.rpm` |
| Windows | [tauri.windows.conf.json](../src-tauri/tauri.windows.conf.json) | `nsis/*-setup.exe` |
| macOS | [tauri.macos.conf.json](../src-tauri/tauri.macos.conf.json) | `macos/*.app`、`dmg/*.dmg` |

普通本地构建使用当前机器的架构。在 macOS 上构建同时支持 Intel 与 Apple Silicon 的通用版：

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
pnpm tauri:build --target universal-apple-darwin -- --locked
```

通用版产物位于 `src-tauri/target/universal-apple-darwin/release/bundle/`。指定其他 `--target` 时也会在 `target/` 下增加对应架构目录。

只生成可执行文件：

```bash
pnpm tauri:build --no-bundle
```

Linux 默认可执行文件为 `src-tauri/target/release/dual-investment`，Windows 为同目录下的 `dual-investment.exe`；macOS 通常使用 `.app` 包运行。

三平台自动构建、版本标签与零付费约束见[发布指南](releases.md)。流水线保留 Releases 草稿，发布前仍需检查构建结果并进行对应平台的运行验证。

## 文档与截图

面向使用者的介绍维护在 [README.md](../README.md) 和 [README.en.md](../README.en.md)，操作细节与计算口径维护在[使用指南](usage.md)，内部实现与检查流程维护在本页。

README 产品图片保存在 `docs/images/`，使用可公开的实际界面截图，不注入或改写行情、订单与收益数据。更新图片时同步检查中英文 README 的说明；浏览器验证生成的临时快照和其他调试产物仍须清理。

## 常见环境问题

- **浏览器提示命令只能在 Tauri 客户端中执行**：使用 `pnpm tauri:dev` 启动桌面客户端；浏览器预览只用于 UI 检查。
- **无法访问系统凭据存储**：检查桌面会话的 Secret Service / keyring 服务是否可用。应用不会降级为明文文件保存凭据。
- **语言加载失败**：界面保留当前语言，启动阶段失败时使用英语；重启应用后重试。构建时应一并保留全部语言资源文件。
