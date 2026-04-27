# AGENTS.md

## 基本协作

- 默认使用中文沟通。
- 本项目只保留实盘路径；不要恢复模式切换、离线占位数据或浏览器数据 fallback。普通浏览器只用于布局检查，缺少 Tauri runtime 时应暴露明确错误。
- 不要把 API Secret、签名 query、Binance 原始响应或敏感上下文写入日志、SQLite、前端 store 或导出文件。API Key/Secret 只允许通过 `src-tauri/src/secrets.rs` 写入系统 keyring。
- 金额、价格、数量、权利金、手续费、保证金和收益率不要用 `f64`；Rust 侧使用 `rust_decimal::Decimal`，前端以字符串承接。
- 所有下单、Simple Earn 申购/赎回、现货-期权划转和到期补齐都是真实资金动作，必须有明确门禁、后端串行保护、恢复记录和脱敏日志。

## 业务边界

- 业务数据只来自 Binance 实盘接口或本地 SQLite 中已记录的真实交易事件；日志只用于排障展示，不作为交易事实来源。
- v1 只支持 `BTC/USDT`。新增标的前必须补齐合约过滤器、保证金估算、风险规则和测试。
- 已保存凭据可在启动和运行中自动连接 Binance；连接失败可静默退避重试，但不得自动开启自动策略、下单或移动资金。凭据缺失、权限不足、账户不可交易等人工错误应暂停自动重连并明确提示。
- 自动策略默认关闭，开启必须二次确认；开启后自动单不逐笔确认，应用重启后自动执行仍保持关闭。存在 `order_attempts` 或 `funding_attempts` 待恢复记录时必须暂停自动执行。
- 接管 Binance 订单是独立于 `order_attempts` 的外部持仓接管功能：只允许从 Binance 当前期权义务仓和 `exchangeInfo` 重建本地跟踪订单，不得把它混入恢复记录，也不得重放历史下单。
- Binance 期权是现金结算；到期 ITM 后现货补齐失败必须进入失败状态，停止相关自动动作并发出风险通知。
- 期权到期、行权和本地合成交割以 Binance `exchangeInfo.expiryDate` 毫秒时间戳为准；订单和请求必须携带精确 `expiryAt`，缺字段是数据契约错误。到期结算价格只来自 Binance 行权记录接口，不得用现货价替代。
- 期权下单价格和数量必须满足 Binance 精度和过滤器字段：价格按 `PRICE_FILTER.tickSize` 对齐，数量按 `LOT_SIZE.stepSize` 校验。
- 默认保证金占用率风控上限为 60%；调整时只改 Rust 侧 `Decimal` 风控规则，并同步更新 README/AGENTS。
- 候选扫描使用 `BTCUSDT` 现货价过滤方向：低买只保留执行价低于现货的 Put，高卖只保留执行价高于现货的 Call；候选浏览范围保持到期 `1-90` 天。
- 下单或补齐需要资金时，先检查现货/期权可用余额；不足时只从 Simple Earn 活期真实持仓赎回，必要时再划转到期权保证金账户。期权保证金准备必须同时覆盖可用保证金缺口和 60% 风控所需的权益缓冲；现货到期权划转确认后，必须重新查询并短时间等待期权账户快照反映到账，再执行可用保证金和占用率风控。等待期间只允许查询，不得重复发送资金动作；超时后按最新账户事实拒绝并归集空闲资金。卖 Put 到期 ITM 的现货补齐 USDT 缓冲由 `SELL_PUT_REBALANCE_USDT_BUFFER` 控制。
- Simple Earn 活期申购/赎回和 Universal Transfer 必须先写入 `funding_attempts` 再发送 Binance；本地只保存可恢复的脱敏事实、交易所引用和状态，不保存签名 query、原始响应或敏感上下文。
- 资金动作请求结果不确定时，不要盲目重发真实资金动作；必须先按 `funding_attempts` 中的 `tranId`、`purchaseId`、`redeemId` 或脱敏匹配条件查询 Binance 历史。历史确认成功后才允许继续后续资金链路；多次查询仍无法唯一确认时必须进入 `blocked` 并阻断自动资金动作。
- `blocked` 资金恢复记录可以在人工核对 Binance 账户事实后解除为 `confirmed` 或 `failed`；解除入口必须要求显式确认，只更新恢复记录和脱敏日志，不得重新发送 Simple Earn 或 Universal Transfer 请求，也不得保存 Binance 原始响应。
- 下单或现货补齐请求结果不确定时，不要盲目重发真实订单；必须先按 `clientOrderId` 或交易所订单号重新查询 Binance，并保留 `order_attempts` 恢复记录。交易所已接单但本地保存失败时，恢复也必须重新查询 Binance 事实。
- 期权订单如果被 Binance 确认为 `EXPIRED`、`REJECTED`、`CANCELED` 或 `CANCELLED` 且成交数量为 0，必须视为下单失败：`order_attempts` 记录为 `failed`，不得创建、恢复或继续跟踪为活跃 `StrategyOrder`，也不得进入同步、结算或现货补齐流程。
- 接管后的订单会进入同步、到期结算和现货补齐流程；接管入口本身不下单、不移动资金，但后续 ITM 补齐仍是真实资金动作，必须保留显式确认和脱敏日志。

## 前端约定

- 前端使用 Vue 3、`<script setup lang="ts">`、Pinia、Naive UI、Vite；跨页面状态放 Pinia，route/view 级组件只做组合。
- 双币赢和自动复投共用 `src/components/views/DualInvestmentView.vue`；候选筛选和下单入口只放在“双币赢”页。不要恢复已删除的离线路径、独立自动复投入口、总览快捷下单、旧风险/收益展示或旧订单快捷动作。
- 订单页可以提供 Binance 当前期权持仓接管入口；接管列表只展示 Binance 查询结果和本地可接管判断，不把接管状态展示成“恢复记录”。恢复中心可以同时展示 `order_attempts` 和 `funding_attempts`，但不得把接管记录混入恢复记录。
- 双币赢下单数量按 Binance 期权“张”计算。候选列表表头写“标记价格”，展示 Binance mark price；右侧下单卡片只允许编辑下单张数。
- 手动下单必须保留明确交互反馈：按钮 loading 按 `order` 维度展示，右侧下单卡片展示校验、资金准备、风控、发送 Binance、保存订单等阶段进度。
- 下单进度事件只承载脱敏阶段摘要、合约号和交易所订单号；不要把进度事件当交易事实来源。
- 导航切换和重复点击通过 Pinia `openView` 刷新页面数据；刷新、后台同步和自动重连不得自动开启策略或下单。静默请求不要清空前台错误提示，并避免 `sync`/`connect` 请求堆积。
- `AppSnapshot.health` 只用于运行观测和提醒，不是订单、结算、资金或日历事实来源。
- 总览指标优先使用后端快照事实口径；前端只做轻量展示派生。日期工具和状态/方向/恢复记录文案集中在 `src/utils/date.ts` 和 `src/utils/labels.ts`。Pinia 对外状态源保持在 `src/stores/app.ts`，可把指标、订单进度、重连等纯展示/状态机 helper 拆到 `src/stores/` 同目录模块。
- 收益明细当前按订单展示权利金、年化和备注，不保留前端硬编码的“收益类型”列。若要展示多种收益来源，必须由 Rust 提供明确收益明细 DTO，不得在前端自行拼分类或从日志推断。
- 收益明细中的订单年化只展示 `StrategyOrder.annualized`，不得在前端用 `Number` 按权利金和到期天数重算真实收益率。
- 总览页和持仓与现货页的账户概览复用 `src/components/common/AccountBalanceList.vue`；Simple Earn 使用真实活期/定期持仓，不使用现货账户 `LD*` 镜像资产。估值由 Rust 计算，无法定价时前端显示“暂无估值”。
- 收益构成和资金构成环形图复用 `src/components/common/DonutChart.vue`；图表项必须使用稳定 key、稳定排序和按标签/资产固定颜色。中心文案展示明确口径（如累计收益、资产估值），悬停或键盘焦点可临时展示单项金额和占比，点击不得产生持久选中状态。
- 候选、订单、义务仓和日历展示到期信息时使用 `expiryAt` 的本地日期时间，不要在前端按日期拼接时间。总览事件日历只从现有快照派生，不从日志文件派生事件。
- 账户、持仓、义务仓等多列列表使用稳定 grid 列宽；标签列左对齐，数字列右对齐并使用等宽数字。

## Rust 与 Tauri 约定

- 金融领域类型集中在 `src-tauri/src/domain.rs`；策略计算放在 `strategy.rs` 及 `src-tauri/src/strategy/` 子模块，状态编排放在 `app_state.rs` 及 `src-tauri/src/app_state/` 子模块。
- Binance 接口只放在 `binance.rs` 及 `src-tauri/src/binance/` 子模块，不要让前端或策略层拼签名；Simple Earn、Universal Transfer、对应历史查询和期权持仓查询也必须在这里封装。
- Binance 查询类 GET 请求可以短退避重试；下单、Simple Earn 申购/赎回和 Universal Transfer 不得在网络失败时盲目重试，结果不确定时先走交易所查询恢复。
- `funding_attempts` 是资金动作恢复事实表，覆盖 `universal_transfer`、`simple_earn_redeem` 和 `simple_earn_subscribe`。`confirmed` 和 `failed` 是终态；`blocked` 需要人工核对 Binance 事实，仍应计入待恢复并阻断新的自动资金动作，直到人工解除为终态。
- `StrategyOrder.source` 是订单跟踪语义的一部分：本机下单/恢复订单使用 `liveOrder`，外部持仓接管订单使用 `adoptedPosition`。不要用 ID 前缀或订单号缺失来推断来源。
- `StrategyOrder.annualized` 是订单快照事实字段，必须由 Rust 使用 `Decimal` 计算并持久化。本机订单使用实际权利金、执行价、实际成交数量、交易所订单创建时间到 `expiryAt` 的天数计算；接管订单使用当前持仓 entry price、数量和 Binance 持仓更新时间到 `expiryAt` 的天数计算。
- 启动加载、快照同步和可跟踪订单判断必须保留 `adoptedPosition`；接管订单可以没有 `exchangeOrderId` 和 `clientOrderId`，不得再用订单号是否存在作为唯一加载条件。
- 接管订单如果没有原始交易所订单号或 `clientOrderId`，同步时只能以 Binance 当前期权持仓和到期行权记录为事实来源；未到期前持仓消失应停止跟踪，已到期后交给结算扫描处理。除非新增历史订单或成交查询并完成可靠匹配，否则不要把接管单的持仓更新时间描述成原始下单时间。
- 系统凭据读写只放在 `secrets.rs`；keyring store 初始化失败不应永久缓存为不可恢复状态，后续应允许重试。
- 文件日志读写只放在 `logs.rs`；日志按 `logs/YYYY-MM-DD.log` 追加 JSONL，只允许脱敏摘要。连接失败可以写脱敏日志事件。
- 所有 command 返回 `Result<T, String>`，内部用 `AppError` 表达错误。订单和恢复记录保存不可变事实，日志只做追加记录。

## 验证要求

- 前端改动先运行 `pnpm lint`，修到无问题后运行 `pnpm typecheck`；涉及依赖、构建、Vite 或 ESLint 配置时再运行 `pnpm build`。需要自动修复时用 `pnpm lint:fix` 后复查 diff。
- Rust 改动先运行 `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`，修到没有问题后再运行 `cargo fmt --manifest-path src-tauri/Cargo.toml`。当前按用户要求暂不保留单元测试代码，交易、结算、风控逻辑改动至少说明未补测试原因。
- Tauri 启动路径、图标、窗口或托盘改动需要实际运行 `pnpm tauri:dev`。
- UI 改动需要检查 1180px 最小桌面宽度下没有文字重叠和表格溢出不可用问题。
