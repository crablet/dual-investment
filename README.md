# 模拟双币赢

一个 Tauri 桌面客户端，用 Binance 期权 + 现货模拟“双币赢”效果，目标是减少交易所打包产品对权利金收益的抽水。

## 功能范围

- 桌面端：Tauri 2 + Vue 3 + Naive UI + Rust。
- 交易范围：v1 仅支持 `BTC/USDT`。
- 策略方向：
  - 卖 Put 抄底：卖出 BTC 看跌期权，到期价低于执行价时自动买入 BTC。
  - 卖 Call 止盈：卖出 BTC 看涨期权，到期价高于执行价时自动卖出 BTC。
- 自动定投：默认每天 `11:30` 扫描，单次 `0.01` 张，使用稳健筛选 preset。
- 后台常驻：系统托盘提供快捷状态和常用操作入口。
- 数据持久化：本地 SQLite 保存策略订单和审计日志。

## 重要风险

币安期权是现金结算，期权本身不会直接把 USDT 换成 BTC 或把 BTC 换成 USDT。这个软件会在到期 ITM 后执行额外的现货补齐订单，才形成类似双币赢的换币结果。

实盘模式会发起真实 Binance 订单。API Key 必须关闭提现权限，仅开启读取账户、读取行情、现货交易和期权交易权限。

## 开发运行

```bash
pnpm install
pnpm tauri:dev
```

只看前端 mock 数据：

```bash
pnpm dev
```

类型检查和构建：

```bash
pnpm typecheck
pnpm build
cd src-tauri && cargo test
```

## 当前实现状态

- `get_app_snapshot` 等 Tauri commands 已连接前后端。
- 浏览器开发环境自动使用 mock 数据，便于调 UI。
- Rust 后端有 Binance EAPI/Spot 签名请求封装，真实期权下单使用受保护限价 + IOC 的快速成交路径。
- 主网自动策略开启需要二次确认；手动主网下单也需要显式确认。
- 到期结算扫描与自动现货补齐调度已在领域模型中定义为核心生命周期，后续应接 Binance 用户数据流和定时任务执行。

## 项目结构

```text
src/
  components/        Vue 页面和通用组件
  composables/       格式化和可复用前端逻辑
  services/          Tauri API 与浏览器 mock
  stores/            Pinia 应用状态
  types/             前后端共享数据形状
src-tauri/
  src/app_state.rs   应用状态、订单与规则编排
  src/binance.rs     Binance 签名请求和订单接口
  src/domain.rs      金融领域模型
  src/persistence.rs SQLite 审计和订单存储
  src/strategy.rs    策略候选、预览和模拟数据
```

## 下一步

- 接入 Binance exchangeInfo 过滤器，按真实合约 tick/step size 校验下单。
- 接入 user data stream，实时同步成交、保证金和订单状态。
- 增加后台调度器，执行到期判定、现货补齐和失败重试。
- 将 API Secret 从内存迁移到系统 keychain 或 Tauri Stronghold。
