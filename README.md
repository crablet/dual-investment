# 双币赢助手

一个 Tauri 桌面客户端，用 Binance 期权 + 现货执行类似“双币赢”的实盘策略，目标是减少交易所打包产品对权利金收益的抽水。

Rust 后端要求 Rust 1.95 或更新版本。

## 功能范围

- 桌面端：Tauri 2 + Vue 3 + Naive UI + Rust。
- 交易范围：v1 仅支持 `BTC/USDT`。
- 策略方向：
  - 卖 Put 抄底：卖出 BTC 看跌期权，到期价低于执行价时自动买入 BTC。
  - 卖 Call 止盈：卖出 BTC 看涨期权，到期价高于执行价时自动卖出 BTC。
- 双币赢工作台：低买/高卖方向选择、候选筛选、按张数下单和自动复投设置在同一页面完成。
- 候选机会：浏览到期 `1-90` 天的 BTC 期权；低买只展示执行价低于现货的 Put，高卖只展示执行价高于现货的 Call。
- 总览事件日历：从订单、自动复投规则、恢复记录、风险提醒和审计事件派生买入、到期、结算、补齐等日程。
- 凭据和数据：API 凭据保存在系统 keyring；SQLite 保存策略订单、订单尝试和审计日志。

## 风险边界

币安期权是现金结算，期权本身不会直接把 USDT 换成 BTC 或把 BTC 换成 USDT。本应用会在到期 ITM 后执行额外的现货补齐订单，才形成类似双币赢的换币结果。

本应用只保留实盘模式，会发起真实 Binance 订单。API Key 必须关闭提现权限，仅开启读取账户、读取行情、现货交易和期权交易权限。

业务数据只来自 Binance 接口或本地 SQLite 中已记录的真实事件。普通浏览器运行只适合检查布局，所有业务命令都需要在 Tauri 客户端内执行。

## 开发运行

```bash
pnpm install
pnpm tauri:dev
```

检查前端布局时可以运行 `pnpm dev`，但普通浏览器没有 Tauri runtime，业务命令会返回“只能在 Tauri 客户端中执行”。

类型检查和构建：

```bash
pnpm typecheck
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
pnpm tauri:build
```

默认 Linux 打包目标是 `deb` 和 `rpm`，产物位于 `src-tauri/target/release/bundle/`。AppImage 当前不作为默认目标。

## 当前状态

- 前后端通过 Tauri commands 连接；浏览器开发环境不提供业务数据 fallback。
- 启动时如果系统 keyring 中已有 Binance 凭据，会尝试自动连接，但不会自动开启自动策略。
- 候选扫描已接入 Binance exchangeInfo、option mark price 和 `BTCUSDT` 现货价；扫描结果保存在运行态快照中。
- 导航页切换和重复点击会按页面刷新所需数据；双币赢页扫描候选，其他业务页同步订单/账户状态。
- 真实期权下单走 Binance 返回的订单号和成交状态；本地保存失败时通过 `order_attempts` 保留恢复线索。
- 总览页事件日历只使用当前快照中的真实订单、规则、审计、风控和恢复记录，不新增后端日历接口或浏览器占位数据。
- 自动策略和手动下单都有确认门禁；自动策略规则可持久化，但应用重启后自动执行保持关闭。

## 项目结构

```text
src/
  components/        Vue 页面和通用组件
    common/EventCalendarBoard.vue  总览事件日历看板
  composables/       格式化和可复用前端逻辑
    useCalendarEvents.ts           从快照派生日历事件
  services/          Tauri API
  stores/            Pinia 应用状态
  types/             前后端共享数据形状
src-tauri/
  src/app_state.rs   应用状态、订单与规则编排
  src/binance.rs     Binance 签名请求和订单接口
  src/domain.rs      金融领域模型
  src/persistence.rs SQLite 审计、订单和订单尝试存储
  src/secrets.rs     系统 keyring 凭据读写
  src/strategy.rs    策略预览和规则默认值
```

## 后续增强

- 接入 user data stream，将当前 60 秒轮询同步升级为更实时的订单和保证金推送。
- 使用真实历史波动率窗口计算 IV Rank。
- 补充成交、结算、现货补齐端到端联调记录。

## 排障

如果 `pnpm tauri:dev` 在 Linux 上启动后出现 `gdk-pixbuf` 的 `data.len() must fit the width, height, and row_stride` panic，优先检查 `src-tauri/icons/icon.png` 是否是有效 RGBA PNG，并确认 Tauri context 缓存已重建。
