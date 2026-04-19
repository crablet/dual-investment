# 双币赢实盘助手

一个 Tauri 桌面客户端，用 Binance 期权 + 现货执行类似“双币赢”的实盘策略，目标是减少交易所打包产品对权利金收益的抽水。

## 功能范围

- 桌面端：Tauri 2 + Vue 3 + Naive UI + Rust。
- 交易范围：v1 仅支持 `BTC/USDT`。
- 策略方向：
  - 卖 Put 抄底：卖出 BTC 看跌期权，到期价低于执行价时自动买入 BTC。
  - 卖 Call 止盈：卖出 BTC 看涨期权，到期价高于执行价时自动卖出 BTC。
- 自动定投：默认每天 `11:30` 扫描，单次 `0.01` 张，使用稳健筛选 preset。
- 后台常驻：系统托盘提供主界面入口。
- 数据持久化：本地 SQLite 保存策略订单和审计日志。

## 重要风险

币安期权是现金结算，期权本身不会直接把 USDT 换成 BTC 或把 BTC 换成 USDT。这个软件会在到期 ITM 后执行额外的现货补齐订单，才形成类似双币赢的换币结果。

本应用只保留实盘模式，会发起真实 Binance 订单。API Key 必须关闭提现权限，仅开启读取账户、读取行情、现货交易和期权交易权限。

业务数据只来自 Binance 实盘接口或本地 SQLite 中已记录的真实事件。浏览器运行只适合检查静态布局，所有业务命令都需要在 Tauri 客户端内执行。

## 开发运行

```bash
pnpm install
pnpm tauri:dev
```

检查前端布局时可以运行 `pnpm dev`，但普通浏览器没有 Tauri runtime，业务命令会返回“只能在 Tauri 实盘客户端中执行”。

如果 `pnpm tauri:dev` 在 Linux 上启动后出现 `gdk-pixbuf` 的 `data.len() must fit the width, height, and row_stride` panic，优先检查 `src-tauri/icons/icon.png`。Tauri 会把这个 PNG 解码成窗口图标的 RGBA 数据，文件必须是有效 RGBA PNG；修改图标后 `src-tauri/build.rs` 会触发 context 缓存重建。

类型检查和构建：

```bash
pnpm typecheck
pnpm build
cd src-tauri && cargo test
```

## 当前实现状态

- `get_app_snapshot` 等 Tauri commands 已连接前后端。
- 浏览器开发环境不再提供前端数据 fallback，需要通过 Tauri 客户端调用后端命令。
- 启动快照在实盘数据同步前返回空持仓、空候选、空收益和空风控状态。
- Rust 后端有 Binance EAPI/Spot 签名请求封装，真实期权下单使用受保护限价 + IOC 的快速成交路径。
- 实盘自动策略开启需要二次确认；手动实盘下单也需要显式确认。
- 候选扫描、账户/保证金同步、到期结算扫描与自动现货补齐需要继续接 Binance 行情、账户和用户数据流。

## 项目结构

```text
src/
  components/        Vue 页面和通用组件
  composables/       格式化和可复用前端逻辑
  services/          Tauri API
  stores/            Pinia 应用状态
  types/             前后端共享数据形状
src-tauri/
  src/app_state.rs   应用状态、订单与规则编排
  src/binance.rs     Binance 签名请求和订单接口
  src/domain.rs      金融领域模型
  src/persistence.rs SQLite 审计和订单存储
  src/strategy.rs    策略预览和实盘规则默认值
```

## 下一步

- 接入 Binance exchangeInfo 过滤器，按真实合约 tick/step size 校验下单。
- 接入 user data stream，实时同步成交、保证金和订单状态。
- 增加后台调度器，执行到期判定、现货补齐和失败重试。
- 将 API Secret 从内存迁移到系统 keychain 或 Tauri Stronghold。
