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
- 账户概览：总览页和持仓与现货页复用同一资产列表；期权保证金、现货和 Binance Simple Earn 持仓在前端按原始 ticker 汇总展示。现货账户里的 `LD*` 镜像资产会被忽略，理财数量来自 Simple Earn 活期/定期持仓接口。估值由后端使用 Binance 现货行情按 `XXXUSDT` 或 `XXXBTC * BTCUSDT` 计算，无法定价时显示“暂无估值”，hover 可查看各来源真实数量。
- 资金准备和归集：下单或到期补齐前检查现货/期权可用余额；不足时从 Simple Earn 活期按 `0.01` 精度向上取整赎回到现货，并在需要时划转到期权保证金账户。卖 Put 到期低买 BTC 时，现货补齐会按结算价乘补齐 BTC 数量再预留 10% USDT 缓冲，用于覆盖市价买入的滑点、手续费和短时价格变化。流程结束后会把不再需要支撑保证金风控的空闲 USDT，以及现货补齐后的 BTC/USDT，重新申购 Simple Earn 活期。
- 手动下单反馈：点击发送订单后，右侧下单卡片会显示校验、资金准备、风控、发送 Binance、保存订单等阶段进度，并在完成或失败时给出全局提示。进度只展示脱敏摘要，真实订单事实仍以策略订单列表和本地订单记录为准。
- 长期运行健康：后台同步会记录最近 tick、成功时间、连续失败次数、跳过轮次和待恢复记录数；总览页会提示后台同步异常和未闭环的恢复记录。健康状态只用于运行观测，不作为交易事实来源。
- 总览事件日历：从订单、自动复投规则、恢复记录和风险提醒派生买入、到期、结算、补齐等日程。
- 凭据和数据：API 凭据保存在系统 keyring；SQLite 保存策略订单、订单尝试和自动规则；每日文件日志保存脱敏运行记录。

## 风险边界

币安期权是现金结算，期权本身不会直接把 USDT 换成 BTC 或把 BTC 换成 USDT。本应用会在到期 ITM 后执行额外的现货补齐订单，才形成类似双币赢的换币结果。

期权到期、行权和本地合成交割使用 Binance `exchangeInfo.expiryDate` 的毫秒时间戳；当前 BTC 期权到期时间为 `08:00:00 UTC`，也就是北京时间 `16:00:00`。订单和请求必须持久化精确 `expiryAt`，缺字段应视为数据契约错误。到期结算价格只使用 Binance 行权记录接口，记录尚未返回时不会用现货价替代，也不会触发现货补齐。

本应用只保留实盘模式，会发起真实 Binance 订单、Simple Earn 活期申购/赎回和现货-期权账户划转。API Key 必须关闭提现权限，仅开启读取账户、读取行情、现货交易、期权交易、Simple Earn 所需的 Spot & Margin Trading 权限，以及 Universal Transfer 权限。

本地风控不设置固定的单笔预计保证金上限；下单前仍会检查合约状态、数量精度与 `LOT_SIZE.stepSize`、价格精度与 `PRICE_FILTER.tickSize`、Binance 当前价格上下限、可用 USDT 保证金、下单后保证金占用率、单日下单次数和连续亏损停止规则。

业务数据只来自 Binance 接口或本地 SQLite 中已记录的真实事件。普通浏览器运行只适合检查布局，所有业务命令都需要在 Tauri 客户端内执行。

本地日志按天写入应用数据目录下的 `logs/YYYY-MM-DD.log`，每行一条 JSON。日志用于排障和设置页展示，只能写脱敏摘要，不能写 API Secret、签名 query 或 Binance 原始响应。

后台同步和手动同步会串行执行，真实下单、到期补齐、Simple Earn 申购/赎回和账户划转也会串行执行，避免长时间运行时因网络慢或重复点击造成同类资金动作重入。Binance 查询类请求会做短退避重试；真实下单和资金移动不会在网络失败时盲目重发。若下单请求结果不确定，应用会优先按 `clientOrderId` 重新查询 Binance，并保留 `order_attempts` 恢复记录。

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
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
pnpm tauri:build
```

默认 Linux 打包目标是 `deb` 和 `rpm`，产物位于 `src-tauri/target/release/bundle/`。AppImage 当前不作为默认目标。

## 当前状态

- 前后端通过 Tauri commands 连接；浏览器开发环境不提供业务数据 fallback。
- 启动时如果系统 keyring 中已有 Binance 凭据，会尝试自动连接，但不会自动开启自动策略。
- 候选扫描已接入 Binance exchangeInfo、option mark price 和 `BTCUSDT` 现货价；期权到期时间保存 `exchangeInfo.expiryDate`，期权价格会按合约 `PRICE_FILTER.tickSize` 对齐，扫描结果保存在运行态快照中。
- 导航页切换和重复点击会按页面刷新所需数据；双币赢页扫描候选，其他业务页同步订单/账户状态。
- 前端静默同步使用自调度退避：成功后恢复 60 秒间隔，失败后逐步退避，已有同步请求进行中时会跳过本轮，避免前端轮询堆积。
- 真实期权下单走 Binance 返回的订单号和成交状态；本地保存失败时通过 `order_attempts` 保留恢复线索。
- 订单会持久化精确 `expiryAt`；恢复订单时必须从当前 `exchangeInfo` 找到同一合约并使用其精确到期时间，查不到则恢复失败。
- 手动下单进度通过 Tauri `order-progress` 事件推送到前端；事件只包含阶段、状态、脱敏文案、合约号和交易所订单号，不包含 Secret、签名 query 或 Binance 原始响应。
- 总览指标优先使用 Rust 快照中的事实口径；前端只做轻量展示派生和格式化。
- 总览页和持仓与现货页的账户概览只在展示层合并同 ticker 资产，并复用同一个资产列表组件；Simple Earn 使用真实活期/定期持仓，不使用现货账户里的 `LD*` 镜像行。估值来自 Rust 快照中的行情换算结果，原始资产代码、来源和数量仍保留在 hover 明细中。
- 资金准备只在手动确认下单、已确认开启的自动策略或到期补齐流程内执行；候选扫描只把现货和可赎回活期 USDT 纳入可用资金判断，不会触发真实资金移动。
- 卖 Put 到期 ITM 后需要低买 BTC 时，现货补齐的 USDT 需求按 `settlement_price * filled_quantity * 1.10` 估算；这只是资金准备缓冲，实际成交仍以 Binance 现货市价单结果为准。
- 期权下单后只归集超过当前保证金风控缓冲的空闲 USDT；到期结算或现货补齐后会再次尝试归集并申购活期，归集失败会写脱敏日志但不会把已创建订单伪装成失败。
- 总览页事件日历只使用当前快照中的真实订单、规则、风控和恢复记录，不新增后端日历接口或浏览器占位数据。
- 设置页可以查看最近日志、打开今日日志文件、打开日志文件夹，并一键删除全部本地日志文件。
- 自动策略和手动下单都有确认门禁；自动策略规则可持久化，但应用重启后自动执行保持关闭。
- 后台健康状态随 `AppSnapshot.health` 返回；待恢复记录存在时健康状态会进入关注态，并阻止自动策略继续执行，直到恢复记录被人工确认或恢复。
- 当前按用户要求暂不保留单元测试代码；后续会专项恢复测试覆盖。专项恢复前，Rust 改动以 `cargo check` 作为最低验证。

## 项目结构

```text
src/
  components/        Vue 页面和通用组件
    common/AccountBalanceList.vue   总览页和持仓页复用的账户资产聚合列表
    common/EventCalendarBoard.vue  总览事件日历看板
  composables/       格式化和可复用前端逻辑
    useCalendarEvents.ts           从快照派生日历事件
    useLogSnapshot.ts              设置页文件日志状态
  services/          Tauri API
  stores/            Pinia 应用状态
  types/             前后端共享数据形状
  utils/             前端纯展示工具
    date.ts          本地日期键、日历时间和计划时间工具
    decimal.ts       Decimal 字符串展示层求和和符号判断
    labels.ts        状态、方向和恢复记录的展示文案映射
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

- 接入 user data stream，将当前 60 秒轮询同步升级为更实时的订单和保证金推送。
- 使用真实历史波动率窗口计算 IV Rank。
- 补充成交、结算、现货补齐端到端联调记录。

## 排障

如果 `pnpm tauri:dev` 在 Linux 上启动后出现 `gdk-pixbuf` 的 `data.len() must fit the width, height, and row_stride` panic，优先检查 `src-tauri/icons/icon.png` 是否是有效 RGBA PNG，并确认 Tauri context 缓存已重建。
