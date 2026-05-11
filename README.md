# 双币赢助手

一个通过 Binance 期权与现货执行 BTC/USDT 双币赢式策略的 Tauri 桌面客户端。所有业务数据和资金动作均使用 Binance 实盘接口。

Rust 后端要求 Rust 1.95 或更新版本。

## 功能

- 在“双币赢”工作台扫描 1–90 天的卖 Put 低买和卖 Call 高卖候选；新的策略只能由用户选择数量、复核后手动发送。
- 汇总期权保证金、现货账户和 Simple Earn 真实持仓，在受控流程内准备下单或到期补齐所需资金。
- 同步订单和持仓，恢复结果不确定的订单与资金请求，并可接管当前 Binance 期权义务仓。
- 使用 Binance 行权记录处理到期结算，必要时通过现货补齐形成换币结果，之后按批次归集闲置 USDT。
- 在总览、恢复中心和设置页展示当前快照、脱敏诊断与日志。

## 风险边界

本应用会发送真实期权订单、现货订单、Simple Earn 申购或赎回，以及现货与期权账户划转。API Key 必须关闭提现权限，只开启读取账户、读取行情、现货交易、期权交易、Simple Earn 和 Universal Transfer 所需权限。

下单前，后端会重新读取订单簿并校验买一量、交易所价格与数量过滤器、可用保证金和占用率。网络结果不确定时不会盲目重发；存在未确认的订单或资金恢复记录时，新的下单和后续资金动作会被阻断。

Binance 期权采用现金结算。应用以 exchangeInfo.expiryDate 派生精确到期时间，并只使用 Binance 行权记录作为结算价格来源。

> 已知安全缺口：手动连接或已保存凭据重连成功后，前端会调用 sync_live_state，后台维护也会继续处理已有恢复、结算、划转、Simple Earn 和到期补齐。因此当前自动重连不是纯只读动作，不能视为“无资金影响”的安全模式。

业务事实只来自 Binance 接口或 SQLite 中记录的真实事件。普通浏览器仅适合布局检查，业务命令必须在 Tauri 客户端中执行；本地日志只写脱敏摘要。

修改订单、资金或恢复流程前，请阅读 [实盘交易安全](docs/trading-safety.md)。

## 收益口径

收益快照以本地订单记录的权利金为基础。卖 Put 到期后若发生现货补齐，另外按 BTCUSDT 当前现货价和补齐成交均价计算所持 BTC 的浮动盈亏；累计收益为权利金与该浮动盈亏之和。

当前模型忽略期权和现货交易手续费，不解析、保存或展示 Binance fee/commission，也不从权利金或浮动盈亏中扣减手续费。

### 年化收益率

annualized 是不计手续费的单利年化估算，不是复利 APY，也不代表已实现收益：

    年化（%） = 权利金 ÷（执行价 × 数量基数）× 365 ÷ 到期天数 × 100

结果保留两位小数。执行价与数量基数的乘积不大于零时返回零；到期天数按两个 UTC 时间之间的秒数向上折算，不足一天按一天计算。

- 候选年化使用扫描时的期权买一价、0.01 张初始数量及合约 unit，从扫描时刻计算到 expiryAt。
- 实盘订单年化使用 Binance 成交均价和实际成交数量；缺失时分别回退到订单价格和请求数量，起点取交易所订单时间。
- 接管订单年化使用 Binance 当前持仓的入场价、持仓数量和持仓更新时间。

前端只展示 Rust 提供的 StrategyCandidate.annualized 或 StrategyOrder.annualized，不重新计算。

## 开发

安装依赖并启动 Tauri：

    pnpm install
    pnpm tauri:dev

只检查前端布局时可运行 pnpm dev；普通浏览器缺少 Tauri runtime，不能执行业务命令。

常规检查与构建：

    pnpm lint
    pnpm typecheck
    pnpm build
    cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
    cargo fmt --manifest-path src-tauri/Cargo.toml
    pnpm tauri:build

## 许可证

项目自有源码、文档和资源使用 GNU Affero General Public License v3.0，SPDX 标识为 AGPL-3.0-only。详见 [LICENSE](LICENSE)。
