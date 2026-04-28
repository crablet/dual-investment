# 双币赢助手

一个 Tauri 桌面客户端，用 Binance 期权 + 现货执行类似“双币赢”的实盘策略，目标是减少交易所打包产品对权利金收益的抽水。

Rust 后端要求 Rust 1.95 或更新版本。

## 功能范围

- 桌面端：Tauri 2 + Vue 3 + Naive UI + Rust。
- 交易范围：v1 仅支持 `BTC/USDT`。
- 策略方向：卖 Put 抄底、卖 Call 止盈；候选范围为到期 `1-90` 天，并按 `BTCUSDT` 现货价过滤方向。
- 双币赢工作台：低买/高卖方向选择、候选筛选、按张数下单和自动复投设置在同一页面完成。
- 账户和资金：后端汇总期权保证金、现货账户和 Binance Simple Earn 真实持仓；下单或到期补齐前检查余额，不足时在受控流程内赎回活期、划转保证金。
- 订单和恢复：期权下单、现货补齐和资金动作都保留可恢复记录；请求结果不确定时重新查询 Binance 事实，存在待恢复记录时暂停新的真实交易和后续资金动作。
- 持仓接管：订单页可接管当前 Binance 期权义务仓，让本机继续同步、到期结算和必要的现货补齐。
- 运行观测：后台同步返回健康状态，设置页可查看脱敏日志；总览事件日历从当前快照派生。
- 凭据连接：API 凭据保存在系统 keyring。已保存凭据可自动重连，但权限错误、凭据缺失等人工错误会暂停自动重连。

## 风险边界

本应用只保留实盘路径，会发起真实 Binance 订单、Simple Earn 活期申购/赎回和现货-期权账户划转。API Key 必须关闭提现权限，只开启读取账户、读取行情、现货交易、期权交易、Simple Earn 所需权限和 Universal Transfer 权限。

Binance 期权是现金结算；到期 ITM 后，应用会通过额外的现货补齐订单形成类似双币赢的换币结果。期权到期和本地合成交割使用 Binance `exchangeInfo.expiryDate` 的毫秒时间戳，到期结算价格只使用 Binance 行权记录接口。

到期扫描按批次处理：一轮扫描会先处理本轮所有到期订单，只有所有可结算订单都明确落定、且不存在待恢复记录时，才会对期权账户闲置 USDT 执行一次 `OPTION_MAIN` 归集并继续申购 Simple Earn。若结算价缺失、现货补齐未完成或恢复记录待确认，本轮归集会被跳过，等待下一轮重新基于 Binance 事实判断。

下单前会校验合约状态、数量 `LOT_SIZE.stepSize`、价格 `PRICE_FILTER.tickSize`、Binance 当前价格上下限、可用保证金和下单后保证金占用率；默认保证金占用率风控上限为 60%。卖 Put 到期低买 BTC 时，现货补齐 USDT 需求按结算价和补齐 BTC 数量预留 10% 缓冲。

真实下单和资金移动不会在网络失败时盲目重发。订单结果不确定时按订单号重新查询 Binance；资金结果不确定时按 `funding_attempts` 中的脱敏事实查询 Binance 历史。多次查询仍无法唯一确认时，记录会进入 `blocked`，需要人工核对 Binance 事实后解除阻断。

恢复中心同时展示订单恢复记录和资金恢复记录。只要仍有未确认恢复记录，应用会阻断自动策略、手动下单和结算中的后续资金动作，避免在事实不清楚时重复执行真实资金操作。

接管 Binance 订单只读取当前期权义务仓并写入本地跟踪订单，不会重放历史下单，也不会立即发送新订单。接管订单可能没有原始交易所订单号，后续同步以 Binance 当前期权持仓和到期行权记录为事实来源；如果到期 ITM，仍可能触发现货补齐。

订单年化收益率是 `StrategyOrder` 的持久化字段，由 Rust 使用权利金、执行价、实际成交或接管持仓数量、订单创建时间和 `expiryAt` 计算。前端只展示该字段，不重新计算真实收益率。

业务数据只来自 Binance 接口或本地 SQLite 中已记录的真实事件。普通浏览器运行只适合检查布局，业务命令需要在 Tauri 客户端内执行。本地日志只写脱敏摘要，不能写 API Secret、签名 query 或 Binance 原始响应。

## 开发运行

```bash
pnpm install
pnpm tauri:dev
```

检查前端布局时可以运行 `pnpm dev`，但普通浏览器没有 Tauri runtime，业务命令会返回“只能在 Tauri 客户端中执行”。

常规检查和构建：

```bash
pnpm lint
pnpm typecheck
pnpm build
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml
pnpm tauri:build
```

需要前端自动修复时运行 `pnpm lint:fix`，再复查 diff。

默认 Linux 打包目标是 `deb` 和 `rpm`，产物位于 `src-tauri/target/release/bundle/`。AppImage 当前不作为默认目标。

## 项目结构

```text
src/
  components/        Vue 页面和通用组件
    common/AccountBalanceList.vue   账户资产聚合列表
    common/EventCalendarBoard.vue   总览事件日历看板
    orders/AdoptOptionPositionsPanel.vue  Binance 期权持仓接管面板
    settings/        Binance 连接和日志设置面板
  composables/       格式化和可复用前端逻辑
    useDualInvestmentCandidates.ts  双币赢候选筛选、排序和选中状态
  services/          Tauri API
  stores/            Pinia 应用状态和展示派生 helper
  types/             前后端共享数据形状
  utils/             前端纯展示工具
src-tauri/
  src/app_state.rs   应用状态、订单与规则编排入口
  src/app_state/     接管、资金、订单恢复、校验和运行时 helper
  src/binance.rs     Binance 客户端和请求入口
  src/binance/       Binance HTTP 辅助和响应解析
  src/domain.rs      金融领域模型
  src/logs.rs        每日文件日志读写
  src/persistence.rs SQLite 订单、订单尝试、资金动作恢复和自动规则存储
  src/secrets.rs     系统 keyring 凭据读写
  src/strategy.rs    策略计算入口
  src/strategy/      候选、订单、余额、风险和 Decimal 工具
```

## 后续增强

- 接入 user data stream，将当前轮询同步升级为更实时的订单和保证金推送。
- 使用真实历史波动率窗口计算 IV Rank。
- 补充成交、结算、现货补齐端到端联调记录。

## 排障

如果 `pnpm tauri:dev` 在 Linux 上启动后出现 `gdk-pixbuf` 的 `data.len() must fit the width, height, and row_stride` panic，优先检查 `src-tauri/icons/icon.png` 是否是有效 RGBA PNG，并确认 Tauri context 缓存已重建。
