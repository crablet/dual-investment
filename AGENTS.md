# AGENTS.md

## 基线

- 默认使用中文沟通。
- 项目只保留 Binance 实盘路径；不要恢复 mock、离线占位、模式切换或浏览器业务 fallback。普通浏览器仅用于布局检查，缺少 Tauri runtime 时应明确报错。
- 新的期权策略只能由用户在“双币赢”页手动发起；不要恢复自动策略、自动下单、系统托盘或其他常驻入口。既有订单仍需完成恢复、结算和资金收尾。
- v1 只支持 BTC/USDT；新增标的前必须补齐合约过滤、保证金估算、风控和验证。
- 业务事实只来自 Binance 实盘接口或 SQLite 中记录的真实事件；日志、进度事件和前端派生状态不是事实源。
- 金额、价格、数量、权利金、保证金和收益率不得使用 f64；Rust 使用 rust_decimal::Decimal，前端以字符串承接。
- 当前收益模型忽略期权和现货手续费；不要引入 fee/commission 字段、保存逻辑或收益扣减。
- API Key/Secret 仅可经 src-tauri/src/secrets.rs 写入系统 keyring。Secret、签名 query、Binance 原始响应及敏感上下文不得进入日志、SQLite、前端状态或导出文件。
- 项目自有源码、文档和资源使用 AGPL-3.0-only，不得引入冲突的许可证声明。
- 开发期不保留旧 IPC、DTO 或 SQLite payload 兼容；结构变更要同步收敛 Rust、TypeScript 和 Pinia，不在应用代码中保留迁移或兼容分支。
- 仓库不保留测试、夹具或快照；临时验证代码、依赖和配置必须在交付前删除。

## 实盘安全

- 下单、Simple Earn、账户划转和到期补齐都是真实资金动作，必须有显式门禁、后端串行保护、恢复记录和脱敏日志。
- 修改连接、后台同步、订单、资金、恢复、接管或结算逻辑前，必须阅读 [实盘交易安全](docs/trading-safety.md)。
- 已保存凭据只能用于重连 Binance，不得触发下单或资金移动；凭据缺失、权限不足等人工错误应暂停自动重连并明确提示。
- 存在未确认的 order_attempts 或 funding_attempts 时，必须阻断手动下单和后续资金动作。
- 手动期权下单必须冻结候选与数量快照并经过应用内复核；用户确认后才允许发送 confirmLive=true。

## 代码边界

- 前端只展示 Rust 快照提供的金融事实，不重新计算年化、估值、收益或恢复状态。跨页面状态放在 src/stores/app.ts，view 组件只做组合。
- 账户资产统一经 src/utils/balances.ts 聚合同名资产，并保留真实账户来源。
- 金融模型和策略计算放在 src-tauri/src/domain.rs 与 src-tauri/src/strategy/；状态编排放在 app_state.rs 与 src-tauri/src/app_state/。
- Binance 通信只放在 binance.rs 与 src-tauri/src/binance/；凭据和文件日志分别只由 secrets.rs 与 logs.rs 处理。

## 验证

- 前端改动运行 pnpm lint、pnpm typecheck；依赖、构建或工具配置变化时再运行 pnpm build。
- Rust 改动运行 cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings，通过后运行 cargo fmt --manifest-path src-tauri/Cargo.toml。
- Tauri 启动、图标或窗口改动实际运行 pnpm tauri:dev；UI 改动检查 1180px 最小宽度下无文字重叠或不可用溢出。
