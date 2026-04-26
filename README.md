# 双币赢助手

一个 Tauri 桌面客户端，用 Binance 期权 + 现货执行类似“双币赢”的实盘策略，目标是减少交易所打包产品对权利金收益的抽水。

Rust 后端要求 Rust 1.95 或更新版本。

## 功能范围

- 桌面端：Tauri 2 + Vue 3 + Naive UI + Rust。
- 交易范围：v1 仅支持 `BTC/USDT`。
- 策略方向：
  - 卖 Put 抄底：卖出 BTC 看跌期权，到期价低于执行价时自动买入 BTC。
  - 卖 Call 止盈：卖出 BTC 看涨期权，到期价高于执行价时自动卖出 BTC。
- 双币赢工作台：低买/高卖方向选择、候选筛选、按张数下单和自动复投设置在同一页面完成。候选范围为到期 `1-90` 天，低买只展示执行价低于现货的 Put，高卖只展示执行价高于现货的 Call。
- 账户和资金：后端汇总期权保证金、现货账户和 Binance Simple Earn 真实持仓；下单或到期补齐前会检查余额，不足时在确认过的交易流程内赎回活期、划转保证金或归集空闲资金。
- 订单、恢复和接管：真实期权下单以 Binance 返回的订单号和状态为准；本地保存失败或请求结果不确定时保留 `order_attempts` 恢复线索，恢复时重新查询 Binance。也可以在订单页接管当前 Binance 期权义务仓，让本机继续同步、到期结算和必要的现货补齐。
- 运行观测：后台同步返回健康状态，设置页可查看脱敏日志；总览事件日历从当前快照中的订单、规则、风险和恢复记录派生。
- 凭据连接：API 凭据保存在系统 keyring。启动或运行中如果已保存凭据暂时连接失败，前端会按退避间隔自动重试；权限错误、凭据缺失等需要人工处理的错误会暂停自动重连。

## 风险边界

本应用只保留实盘路径，会发起真实 Binance 订单、Simple Earn 活期申购/赎回和现货-期权账户划转。API Key 必须关闭提现权限，只开启读取账户、读取行情、现货交易、期权交易、Simple Earn 所需权限和 Universal Transfer 权限。

币安期权是现金结算；到期 ITM 后，本应用会通过额外的现货补齐订单形成类似双币赢的换币结果。期权到期、行权和本地合成交割使用 Binance `exchangeInfo.expiryDate` 的毫秒时间戳；到期结算价格只使用 Binance 行权记录接口，不用现货价替代。

下单前会校验合约状态、数量 `LOT_SIZE.stepSize`、价格 `PRICE_FILTER.tickSize`、Binance 当前价格上下限、可用保证金和下单后保证金占用率；默认保证金占用率风控上限为 60%。卖 Put 到期低买 BTC 时，现货补齐 USDT 需求按结算价和补齐 BTC 数量预留 10% 缓冲。

Binance 查询类请求会做短退避重试；真实下单和资金移动不会在网络失败时盲目重发。请求结果不确定时，应用会先按 `clientOrderId` 或交易所订单号重新查询 Binance。

接管 Binance 订单只读取当前期权持仓并写入本地跟踪订单，不会重放历史下单，也不会立即发送新订单。接管后的订单会持久化到本地 SQLite，应用重启后仍会继续加载、同步和等待到期结算；如果到期 ITM，仍可能触发真实现货补齐。

使用接管功能时，在订单页刷新 Binance 当前期权持仓，选中可接管的卖出期权义务仓并确认“接管选中”。已经被本地跟踪、不是 `BTC/USDT`、不是卖出义务仓、已到期或缺少有效入场价的持仓不会允许接管。

本地订单会记录来源：本机下单/恢复订单标记为 `liveOrder`，外部持仓接管订单标记为 `adoptedPosition`。接管订单可能没有原始交易所订单号，后续同步以 Binance 当前期权持仓和到期行权记录为事实来源。

业务数据只来自 Binance 接口或本地 SQLite 中已记录的真实事件。普通浏览器运行只适合检查布局，所有业务命令都需要在 Tauri 客户端内执行。本地日志只写脱敏摘要，不能写 API Secret、签名 query 或 Binance 原始响应。

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
  composables/       格式化和可复用前端逻辑
  services/          Tauri API
  stores/            Pinia 应用状态
  types/             前后端共享数据形状
  utils/             前端纯展示工具
src-tauri/
  src/app_state.rs   应用状态、订单与规则编排
  src/binance.rs     Binance 签名请求和订单接口
  src/domain.rs      金融领域模型
  src/logs.rs        每日文件日志读写
  src/persistence.rs SQLite 订单、订单尝试和自动规则存储
  src/secrets.rs     系统 keyring 凭据读写
  src/strategy.rs    策略预览和规则默认值
```

## 后续增强

- 接入 user data stream，将当前轮询同步升级为更实时的订单和保证金推送。
- 使用真实历史波动率窗口计算 IV Rank。
- 补充成交、结算、现货补齐端到端联调记录。

## 排障

如果 `pnpm tauri:dev` 在 Linux 上启动后出现 `gdk-pixbuf` 的 `data.len() must fit the width, height, and row_stride` panic，优先检查 `src-tauri/icons/icon.png` 是否是有效 RGBA PNG，并确认 Tauri context 缓存已重建。
