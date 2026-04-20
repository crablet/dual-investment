# 双币赢助手

一个 Tauri 桌面客户端，用 Binance 期权 + 现货执行类似“双币赢”的策略，目标是减少交易所打包产品对权利金收益的抽水。

Rust 后端要求 Rust 1.95 或更新版本。

## 功能范围

- 桌面端：Tauri 2 + Vue 3 + Naive UI + Rust。
- 交易范围：v1 仅支持 `BTC/USDT`。
- 策略方向：
  - 卖 Put 抄底：卖出 BTC 看跌期权，到期价低于执行价时自动买入 BTC。
  - 卖 Call 止盈：卖出 BTC 看涨期权，到期价高于执行价时自动卖出 BTC。
- 双币赢工作台：同一页面完成低买/高卖方向选择、候选筛选、按张数下单和自动复投设置。
- 候选机会：浏览范围为到期 `1-90` 天；低买只展示执行价低于 `BTCUSDT` 现货价的 Put，高卖只展示执行价高于现货价的 Call，并标出目标价距离现货的百分比。前端候选表可按年化、被行权概率和距现货范围筛选，不依赖后端截断来减少展示量。
- 价格字段：候选列表和下单表单中的“价格”是期权权利金单价；预计权利金按 `价格 × 张数` 计算。
- 自动复投：默认每天 `11:30` 扫描，单次 `0.01` 张，使用稳健筛选 preset。
- 后台常驻：系统托盘提供主界面入口。
- 数据持久化：Tauri 应用数据目录中的 SQLite 保存策略订单、订单尝试和审计日志；API 凭据通过 `keyring-core` 保存在系统 keyring。
- 已保存凭据：启动时会自动读取系统 keyring 中的 Binance 凭据并尝试连接；不会自动开启自动策略。

## 重要风险

币安期权是现金结算，期权本身不会直接把 USDT 换成 BTC 或把 BTC 换成 USDT。这个软件会在到期 ITM 后执行额外的现货补齐订单，才形成类似双币赢的换币结果。

本应用只保留实盘模式，会发起真实 Binance 订单。API Key 必须关闭提现权限，仅开启读取账户、读取行情、现货交易和期权交易权限。

业务数据只来自 Binance 接口或本地 SQLite 中已记录的真实事件。浏览器运行只适合检查静态布局，所有业务命令都需要在 Tauri 客户端内执行。

## 开发运行

```bash
pnpm install
pnpm tauri:dev
```

检查前端布局时可以运行 `pnpm dev`，但普通浏览器没有 Tauri runtime，业务命令会返回“只能在 Tauri 客户端中执行”。

如果 `pnpm tauri:dev` 在 Linux 上启动后出现 `gdk-pixbuf` 的 `data.len() must fit the width, height, and row_stride` panic，优先检查 `src-tauri/icons/icon.png`。Tauri 会把这个 PNG 解码成窗口图标的 RGBA 数据，文件必须是有效 RGBA PNG；修改图标后 `src-tauri/build.rs` 会触发 context 缓存重建。

类型检查和构建：

```bash
pnpm typecheck
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
pnpm tauri:build
```

默认 Linux 打包目标是 `deb` 和 `rpm`，产物位于 `src-tauri/target/release/bundle/`。AppImage 当前不作为默认目标：Tauri bundler 使用的 `linuxdeploy` 在部分较新的 Linux 系统上会因为旧版 `strip` 无法识别 `.relr.dyn` ELF section 而失败。

## 当前实现状态

- `get_app_snapshot` 等 Tauri commands 已连接前后端。
- 浏览器开发环境不再提供前端数据 fallback，需要通过 Tauri 客户端调用后端命令。
- 启动快照在未连接 API 前返回空持仓、空候选、空收益和空风控状态；如果系统 keyring 中已有 Binance 凭据，前端启动流程会自动尝试连接并同步 Binance 状态。
- Rust 后端有 Binance EAPI/Spot 签名请求封装，连接时会真实校验期权账户和现货交易权限，下单前会读取 exchangeInfo、mark price、账户保证金并执行合约过滤器和风控规则校验。
- 候选扫描已接入 Binance exchangeInfo、option mark price 和 `BTCUSDT` 现货价，按方向、合约状态、浏览到期范围和数量范围返回全部命中候选；自动复投规则只影响“推荐”评级，不决定候选是否可见。扫描结果和诊断统计保存在运行态快照中，后续订单/账户同步不会把候选列表清空。
- 打开双币赢工作台时，如果 Binance 已连接且当前没有其他同步任务，前端会自动扫描一次候选；右上角刷新按钮仍可用于手动重新扫描。
- 真实期权下单使用用户填写的价格 + IOC 的快速成交路径，订单入库使用 Binance 返回的 `orderId`、`clientOrderId`、成交数量、均价和交易所状态；零成交、部分成交、拒绝、过期会分别落到不同本地状态。
- SQLite 会记录订单尝试阶段，前端订单页会展示恢复记录，并可从 Binance 查询真实订单来恢复“交易所已创建订单但本地保存失败”的本地状态。
- 后台调度器会周期性同步订单状态、扫描到期订单；到期 ITM 后会按方向发送 Binance Spot 市价补齐订单，失败时进入 `rebalanceFailed` 并写审计事件。
- 自动策略开启需要二次确认；手动下单也需要显式确认。
- 自动策略规则会持久化；应用重启后规则仍保留，但自动执行默认关闭，必须重新二次确认开启。开启后会按规则每日触发一次扫描，也可以在双币赢工作台手动执行一次自动扫描。
- Binance API Key/Secret 通过 `keyring-core` + 平台 store 保存和读取；Linux 使用 DBus Secret Service，macOS 使用登录钥匙串，Windows 使用 Credential Manager。保存失败时不会建立连接；已保存凭据失效时会暴露连接错误，不会使用前端 fallback。

## 项目结构

```text
src/
  components/        Vue 页面和通用组件
    views/DualInvestmentView.vue  双币赢工作台，包含候选列表、手动下单和自动复投设置
  composables/       格式化和可复用前端逻辑
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
- 使用真实历史波动率窗口计算 IV Rank，目前候选筛选仍以 Binance mark IV 和 delta 为主。
- 补充更完整的成交、结算、现货补齐端到端联调记录。
