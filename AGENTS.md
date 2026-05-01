# AGENTS.md

## 核心原则

- 默认使用中文沟通。
- 只保留 Binance 实盘路径；不要恢复 mock、离线占位数据、模式切换或浏览器业务 fallback。普通浏览器只用于布局检查，缺少 Tauri runtime 时应明确报错。
- 不要把 API Secret、签名 query、Binance 原始响应或敏感上下文写入日志、SQLite、前端 store 或导出文件。API Key/Secret 只允许通过 src-tauri/src/secrets.rs 写入系统 keyring。
- 金额、价格、数量、权利金、手续费、保证金和收益率不得用 f64；Rust 使用 rust_decimal::Decimal，前端以字符串承接。
- 业务事实只来自 Binance 实盘接口或本地 SQLite 中已记录的真实事件；日志、进度事件和前端派生状态只用于展示或排障。
- 开发阶段不维护旧 IPC、旧前端 DTO 或旧 SQLite JSON payload 兼容；删字段时同步 Rust DTO、TypeScript 类型和 Pinia 状态，不用 serde(default)、别名字段或前端 fallback 保留旧结构。不要在应用代码里保留开发期 SQLite 迁移、清表或兼容分支；本地测试库只用一次性命令或临时脚本处理，脚本不要提交。

## 交易安全

- v1 只支持 BTC/USDT；新增标的前必须补齐合约过滤器、保证金估算、风险规则和验证。
- 所有下单、Simple Earn 申购/赎回、现货-期权划转和到期补齐都是真实资金动作，必须有显式门禁、后端串行保护、恢复记录和脱敏日志。
- 已保存凭据只能自动重连 Binance，不得自动开启策略、下单或移动资金；凭据缺失、权限不足等人工错误应暂停自动重连并明确提示。自动策略默认关闭，开启必须二次确认，重启后仍保持关闭。
- 存在 order_attempts 或 funding_attempts 待恢复记录时，必须阻断自动执行、手动下单和后续资金动作。
- 手动期权下单最终门禁必须是应用内复核弹窗：冻结用户发送订单时的候选和数量快照，展示足够确认方向、合约、价格、数量、到期、预计收益和风险的事实，用户确认后才允许传 confirmLive=true。
- 候选扫描使用 BTCUSDT 现货价过滤方向：低买只保留执行价低于现货的 Put，高卖只保留执行价高于现货的 Call；候选到期范围保持 1-90 天。
- 期权订单和请求必须携带 Binance exchangeInfo.expiryDate 派生的精确 expiryAt；到期结算价格只来自 Binance 行权记录接口，不得用现货价替代。
- 期权下单价格和数量必须满足 Binance PRICE_FILTER.tickSize 与 LOT_SIZE.stepSize；默认保证金占用率风控上限为 60%。
- 下单或补齐需要资金时，只能基于现货/期权余额和 Simple Earn 真实持仓准备资金；请求确认后等待账户快照反映到账，等待期间只允许查询，不得重复发送资金动作。
- 到期结算后的期权闲置 USDT 归集是批次级收尾动作：结算批次完成后创建 settlement_sweeps 任务并延迟约 5 分钟执行；不得按每个订单重复发送 OPTION_MAIN；同一 chain_id 已确认的划转/申购必须续跑下一步，不得重复发送；低于 10 USDT 不自动归集；存在待恢复记录、未完成结算或未完成补齐时跳过或延后归集。
- OPTION_MAIN 确认成功后必须等待现货账户快照反映到账，才能申购 Simple Earn；等待期间只允许查询，不得重复发送划转或申购请求。

## 恢复与接管

- 下单或现货补齐结果不确定时，不要盲目重发真实订单；必须按 clientOrderId 或交易所订单号重新查询 Binance，并保留 order_attempts 恢复记录。
- 期权订单若被 Binance 确认为 EXPIRED、REJECTED、CANCELED 或 CANCELLED 且成交数量为 0，必须标记失败，不得创建、恢复或继续跟踪为活跃 StrategyOrder。
- Simple Earn 和 Universal Transfer 必须先写入 funding_attempts 再发送 Binance；结果不确定时只按交易所引用或同类型、同资产、同金额的唯一历史记录确认，不唯一则进入 blocked。
- 资金恢复查询失败只记录为临时错误并按慢退避重查，不计入有效“查不到”次数；只有 Binance 历史接口成功返回但仍无法确认时，才增加 effective_query_count。恢复记录应保留脱敏诊断字段：查询窗口、下次查询时间、有效查询次数、临时错误次数和错误分类。
- blocked 资金记录只能在人工核对 Binance 事实后解除为 confirmed 或 failed；解除操作只更新恢复记录和脱敏日志，不得重新发送资金请求。
- Binance 期权持仓接管只允许从当前义务仓和 exchangeInfo 创建本地跟踪订单；接管入口不下单、不移动资金、不混入恢复记录。接管订单使用 StrategyOrder.source = adoptedPosition，后续同步以当前持仓和到期行权记录为事实来源。

## 前端与后端边界

- 前端使用 Vue 3、script setup、Pinia、Naive UI、Vite；跨页面状态源保持在 src/stores/app.ts，route/view 级组件只做组合。
- Naive UI 全局主题配置放在 src/theme/；App.vue 只负责 provider、视图组合和应用级 wiring，不堆叠主题常量或业务逻辑。
- 双币赢和自动复投共用 src/components/views/DualInvestmentView.vue；候选筛选和下单入口只放在“双币赢”页。订单页可以提供持仓接管入口，恢复中心可以展示恢复记录，但不得把接管状态展示成恢复记录。
- 前端只做展示派生：订单年化只展示 StrategyOrder.annualized，收益分类不得从日志推断，账户估值由 Rust 提供，无法定价时显示“暂无估值”。下单进度事件、日志和图表交互不得作为交易事实来源。
- 账户资产展示统一复用 src/utils/balances.ts 聚合同名资产；页面不得直接遍历原始 balances 后把来源硬编码成“现货”。同名资产可合并展示，但 tooltip 或明细必须保留真实账户来源。
- 到期信息统一展示 expiryAt 的本地日期时间；总览事件日历只从快照派生。
- 金融领域类型集中在 src-tauri/src/domain.rs；策略计算在 strategy.rs 和 src-tauri/src/strategy/；状态编排在 app_state.rs 和 src-tauri/src/app_state/。
- Binance 请求、签名、Simple Earn、Universal Transfer、历史查询和期权持仓查询只放在 binance.rs 及 src-tauri/src/binance/。系统凭据读写只放在 secrets.rs；文件日志读写只放在 logs.rs，日志按 logs/YYYY-MM-DD.log 追加 JSONL，只允许脱敏摘要。
- 所有 command 返回 Result<T, String>，内部用 AppError 表达错误。订单和恢复记录保存可恢复事实，日志只做追加记录。

## 验证要求

- 前端改动先运行 pnpm lint，修到无问题后运行 pnpm typecheck；涉及依赖、构建、Vite 或 ESLint 配置时再运行 pnpm build。
- Rust 改动先运行 cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings，修到没有问题后再运行 cargo fmt --manifest-path src-tauri/Cargo.toml。
- Tauri 启动路径、图标、窗口或托盘改动需要实际运行 pnpm tauri:dev。
- UI 改动需要检查 1180px 最小桌面宽度下没有文字重叠和表格溢出不可用问题。
