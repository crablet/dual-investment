# AGENTS.md

## 基本协作

- 默认使用中文沟通。
- 本项目只保留实盘路径；不要恢复模式切换、离线占位数据或浏览器数据 fallback。
- 普通浏览器运行只用于布局检查；缺少 Tauri runtime 时应暴露明确错误。
- 不要把真实 API Secret、签名 query、订单响应原文写入日志或前端状态。
- API Key/Secret 只允许通过 `src-tauri/src/secrets.rs` 写入系统 keyring；不要保存到 SQLite、日志文件、前端 store 或导出文件。
- 任何下单功能都按真实下单处理，必须有显式确认门禁和脱敏日志事件。
- 不要用 `f64` 表示金额、价格、数量、权利金、手续费或收益率；Rust 侧使用 `rust_decimal::Decimal`，前端以字符串承接。

## 业务边界

- 业务数据只允许来自 Binance 实盘接口或本地 SQLite 中已记录的真实交易事件；日志文件只用于排障展示，不作为交易事实来源。
- v1 只支持 `BTC/USDT`；新增标的必须补齐合约过滤器、保证金估算、风险规则和测试。
- 已保存凭据可在应用启动时自动读取并连接 Binance，但不得自动开启自动策略。
- 自动策略默认稳健且必须二次确认开启；开启后自动单不逐笔确认，应用重启后自动执行仍保持关闭。
- 币安期权是现金结算；到期 ITM 后现货补齐失败必须进入失败状态，停止相关自动动作并发出风险通知。
- 交易所已接单但本地订单保存失败时，必须保留 `order_attempts` 恢复记录和交易所订单号；恢复时必须重新查询 Binance 真实订单，不要凭本地记录凭空创建订单。
- 期权到期、行权和本地合成交割必须以 Binance `exchangeInfo.expiryDate` 毫秒时间戳为准；当前 BTC 期权为到期日 `08:00:00 UTC`（北京时间 `16:00:00`）。订单和请求必须携带精确 `expiryAt`，缺字段应视为数据契约错误。
- 到期结算价格必须来自 Binance 行权记录接口；记录尚未出现或接口不可用时不得用现货价替代结算价，不得触发现货补齐。
- 期权下单价格和数量必须同时满足 Binance `exchangeInfo` 中的精度字段和过滤器字段；价格按 `PRICE_FILTER.tickSize` 对齐，数量按 `LOT_SIZE.stepSize` 校验，不要只检查 `priceScale`/`quantityScale`。
- 下单或到期补齐需要资金时，先检查现货/期权保证金可用余额；不足时只允许从 Simple Earn 活期真实持仓赎回，赎回金额按 `0.01` 向上取整。需要期权保证金时再走现货到账户划转；流程结束后只归集不再支撑保证金风控缓冲的空闲资金，并重新申购 Simple Earn 活期。
- 不要恢复固定的单笔预计保证金上限；下单风险由可用保证金、下单后保证金占用率、单日下单次数和连续亏损停止等规则控制。
- Simple Earn 活期申购/赎回和现货-期权划转都是真实资金移动，必须在已有下单确认、自动策略确认或到期补齐流程内执行，并写脱敏日志事件；归集失败不得篡改已创建订单事实。
- 候选扫描必须用 `BTCUSDT` 现货价过滤方向：低买只保留执行价低于现货的 Put，高卖只保留执行价高于现货的 Call。
- 候选浏览范围保持为到期 `1-90` 天；自动复投规则只影响推荐评级，不决定候选是否可见。候选扫描应返回足够排查过滤漏斗的诊断统计。

## 前端约定

- 前端使用 Vue 3、`<script setup lang="ts">`、Pinia、Naive UI、Vite；跨页面状态放 Pinia，route/view 级组件只做组合。
- 双币赢和自动复投共用 `src/components/views/DualInvestmentView.vue`；不要恢复独立自动复投视图或单独导航入口。
- 双币赢下单数量按 Binance 期权“张”计算，不要改成按 USDT 金额申购。
- 候选列表中期权权利金单价表头写“标记价格”，展示 Binance mark price；实际下单限价仍由 Rust 按 `PRICE_FILTER.tickSize` 对齐。右侧下单卡片只允许编辑下单张数，交易参数仍按原始字符串精度处理。
- 手动“发送订单”必须保留明确交互反馈：按钮 loading 按 `order` 维度展示，右侧下单卡片展示校验、资金准备、风控、发送 Binance、保存订单等阶段进度，并在成功/失败时给出全局提示。
- 下单进度事件只允许承载脱敏阶段摘要、合约号和交易所订单号；不要把进度事件当交易事实来源，也不要在其中放 API Secret、签名 query 或 Binance 原始响应。
- 导航页切换和重复点击应通过 Pinia 的 `openView` 入口触发对应页面刷新；刷新只拉取数据，不得自动开启自动策略或自动下单。
- 后台轮询同步应使用静默请求，不要清空前台错误提示；按钮 loading 按操作维度展示。
- 总览指标优先使用后端快照中的事实口径，例如“今日权利金”使用 `pnl.today`；不要在前端按订单日期重复计算事实口径。
- 前端轻量展示派生保留在 TypeScript；不要为了格式化、标签或小型 UI 聚合新增 IPC command。
- 总览页和持仓与现货页的账户概览复用 `src/components/common/AccountBalanceList.vue`，按原始 ticker 做展示层聚合；Binance Simple Earn 必须使用活期/定期持仓接口中的真实资产数量，不要用现货账户里的 `LD*` 镜像资产。估值由 Rust 使用 Binance 现货行情按 `XXXUSDT` 或 `XXXBTC * BTCUSDT` 计算，无法定价时前端应显示“暂无估值”，hover 明细保留原始资产代码、来源和真实数量；不要恢复手写的旧“现货持仓”列表。
- 日期工具和状态/方向/恢复记录文案映射集中在 `src/utils/date.ts` 和 `src/utils/labels.ts`，不要在组件或 composable 内重复维护。
- 候选、订单、持仓义务和日历展示到期信息时使用 `expiryAt` 的本地日期时间；缺少 `expiryAt` 是数据契约错误，不要在前端按日期拼接时间。
- 总览事件日历使用 `src/components/common/EventCalendarBoard.vue` 和 `src/composables/useCalendarEvents.ts`，只从现有快照派生事件；不要新增独立导航入口、浏览器占位日程或手动维护日程。
- 日历可展示订单、到期、结算、补齐、自动复投、风险和恢复事件；不要从日志文件派生日历事件或把敏感上下文铺到前端。
- 账户、持仓、义务仓这类多列列表使用稳定 grid 列宽；标签列左对齐，数字列右对齐并使用等宽数字。

## Rust 与 Tauri 约定

- 金融领域类型集中在 `src-tauri/src/domain.rs`。
- Binance 接口只放在 `binance.rs`，不要让前端或策略层拼签名；Simple Earn 活期申购/赎回和 Universal Transfer 也必须在这里封装。
- 系统凭据读写只放在 `secrets.rs`，调用方只接收连接状态或脱敏 API Key。
- 策略计算放在 `strategy.rs`，状态编排放在 `app_state.rs`。
- 文件日志读写只放在 `logs.rs`；日志按 `logs/YYYY-MM-DD.log` 每日 JSONL 追加写入，只允许脱敏摘要。
- 所有 command 返回 `Result<T, String>`，内部用 `AppError` 表达错误。
- 订单和恢复记录应保存不可变事实；日志只做追加记录，不要覆盖历史事实。
- `src-tauri/icons/icon.png` 和 Linux bundle 目标影响启动与打包；除非任务需要，不要改图标资源或把默认 `deb`/`rpm` 目标改回 `all`。

## 验证要求

- 前端改动至少运行 `pnpm typecheck`。
- Rust 改动至少运行 `cargo test` 或 `cargo check`。
- Tauri 启动路径、图标、窗口或托盘改动需要实际运行 `pnpm tauri:dev`。
- 当前按用户要求暂不保留单元测试代码；专项恢复前不要新增测试，修改交易、结算、风控逻辑至少运行 `cargo check` 并说明未补测试的原因。
- UI 改动需要检查 1180px 最小桌面宽度下没有文字重叠和表格溢出不可用问题。
