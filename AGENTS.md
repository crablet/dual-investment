# AGENTS.md

## 基本协作

- 默认使用中文沟通。
- 本项目只保留实盘路径；不要恢复模式切换、离线占位数据或浏览器数据 fallback。
- 普通浏览器运行只用于布局检查；缺少 Tauri runtime 时应暴露明确错误。
- 不要把真实 API Secret、签名 query、订单响应原文写入日志或前端状态。
- API Key/Secret 只允许通过 `src-tauri/src/secrets.rs` 写入系统 keyring；不要保存到 SQLite、审计日志、前端 store 或导出文件。
- 任何下单功能都按真实下单处理，必须有显式确认门禁和审计事件。
- 不要用 `f64` 表示金额、价格、数量、权利金、手续费或收益率；Rust 侧使用 `rust_decimal::Decimal`，前端以字符串承接。

## 业务边界

- 业务数据只允许来自 Binance 实盘接口或本地 SQLite 中已记录的真实事件。
- v1 只支持 `BTC/USDT`；新增标的必须补齐合约过滤器、保证金估算、风险规则和测试。
- 已保存凭据可在应用启动时自动读取并连接 Binance，但不得自动开启自动策略。
- 自动策略默认稳健且必须二次确认开启；开启后自动单不逐笔确认，应用重启后自动执行仍保持关闭。
- 币安期权是现金结算；到期 ITM 后现货补齐失败必须进入失败状态，停止相关自动动作并发出风险通知。
- 交易所已接单但本地订单保存失败时，必须保留 `order_attempts` 恢复记录和交易所订单号；恢复时必须重新查询 Binance 真实订单，不要凭本地记录凭空创建订单。
- 候选扫描必须用 `BTCUSDT` 现货价过滤方向：低买只保留执行价低于现货的 Put，高卖只保留执行价高于现货的 Call。
- 候选浏览范围保持为到期 `1-90` 天；自动复投规则只影响推荐评级，不决定候选是否可见。候选扫描应返回足够排查过滤漏斗的诊断统计。

## 前端约定

- 前端使用 Vue 3、`<script setup lang="ts">`、Pinia、Naive UI、Vite；跨页面状态放 Pinia，route/view 级组件只做组合。
- 双币赢和自动复投共用 `src/components/views/DualInvestmentView.vue`；不要恢复独立自动复投视图或单独导航入口。
- 双币赢下单数量按 Binance 期权“张”计算，不要改成按 USDT 金额申购。
- 候选和下单 UI 中期权权利金单价统一写成“价格”；右侧下单卡片只允许编辑下单张数，交易参数仍按原始字符串精度处理。
- 导航页切换和重复点击应通过 Pinia 的 `openView` 入口触发对应页面刷新；刷新只拉取数据，不得自动开启自动策略或自动下单。
- 后台轮询同步应使用静默请求，不要清空前台错误提示；按钮 loading 按操作维度展示。
- 总览事件日历使用 `src/components/common/EventCalendarBoard.vue` 和 `src/composables/useCalendarEvents.ts`，只从现有快照派生事件；不要新增独立导航入口、浏览器占位日程或手动维护日程。
- 日历可展示订单、到期、结算、补齐、自动复投、风险、恢复和系统事件；审计事件必须按 allowlist 映射，不要把未知审计消息或敏感上下文原样铺到前端。
- 账户、持仓、义务仓这类多列列表使用稳定 grid 列宽；标签列左对齐，数字列右对齐并使用等宽数字。

## Rust 与 Tauri 约定

- 金融领域类型集中在 `src-tauri/src/domain.rs`。
- Binance 接口只放在 `binance.rs`，不要让前端或策略层拼签名。
- 系统凭据读写只放在 `secrets.rs`，调用方只接收连接状态或脱敏 API Key。
- 策略计算放在 `strategy.rs`，状态编排放在 `app_state.rs`。
- 所有 command 返回 `Result<T, String>`，内部用 `AppError` 表达错误。
- 订单、审计、资金变更应写不可变事件；不要覆盖历史事实。
- `src-tauri/icons/icon.png` 和 Linux bundle 目标影响启动与打包；除非任务需要，不要改图标资源或把默认 `deb`/`rpm` 目标改回 `all`。

## 验证要求

- 前端改动至少运行 `pnpm typecheck`。
- Rust 改动至少运行 `cargo test` 或 `cargo check`。
- Tauri 启动路径、图标、窗口或托盘改动需要实际运行 `pnpm tauri:dev`。
- 修改交易、结算、风控逻辑时必须补单元测试。
- UI 改动需要检查 1180px 最小桌面宽度下没有文字重叠和表格溢出不可用问题。
