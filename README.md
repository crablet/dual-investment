# 双币赢助手

一个通过 Binance 期权与现货执行 BTC/USDT 双币赢式策略的 Tauri 桌面客户端。所有业务数据和资金动作均使用 Binance 实盘接口。

Rust 后端要求 Rust 1.95 或更新版本。

## 功能

- 在“双币赢”工作台扫描 1–90 天的卖 Put 低买和卖 Call 高卖候选；新的策略只能由用户选择数量、复核后手动发送。
- 汇总期权保证金、现货账户和 Simple Earn 真实持仓，在受控流程内准备下单或到期补齐所需资金。
- 同步订单和持仓，恢复结果不确定的订单与资金请求，并可接管当前 Binance 期权义务仓。
- 使用 Binance 行权记录处理到期结算，必要时通过现货补齐形成换币结果，之后按批次归集闲置 USDT。
- 总览集中展示关键指标、账户概览、活跃策略和事件日历；收益报表、恢复记录与脱敏日志分别在“收益分析”“策略订单”和“设置”页查看。

## 账户资产口径

v1 的新策略只支持 BTC/USDT，但账户页面会展示 Binance 返回的其他真实资产。同名资产按代码聚合并保留账户来源；Simple Earn 镜像只在同轮快照精确匹配时去重。缺少有效价格的资产仍展示真实数量，但不计入估值合计。

## 风险边界

本应用会发送真实期权订单、现货订单、Simple Earn 申购或赎回，以及现货与期权账户划转。API Key 必须关闭提现权限，只开启读取账户、读取行情、现货交易、期权交易、Simple Earn 和 Universal Transfer 所需权限。

下单前，后端会重新读取订单簿并校验交易所过滤器、买一量和保证金。结果不确定时不会盲目重发；未确认的订单或资金记录会阻断后续动作。

Binance 期权采用现金结算。应用以 exchangeInfo.expiryDate 派生精确到期时间，并只使用 Binance 行权记录作为结算价格来源。

> 已知安全缺口：当前保存凭据重连可能推进已有订单和资金流程，不能视为纯只读连接。

修改订单、资金或恢复流程前，请阅读 [实盘交易安全](docs/trading-safety.md)。

## 收益口径

收益快照以本地订单记录的权利金为基础。卖 Put 到期后若发生现货补齐，另外按 BTCUSDT 当前现货价和补齐成交均价计算所持 BTC 的浮动盈亏；累计收益为权利金与该浮动盈亏之和。

当前模型忽略期权和现货交易手续费，不解析、保存或展示 Binance fee/commission，也不从权利金或浮动盈亏中扣减手续费。

### 年化收益率

annualized 是不计手续费的单利年化估算，不是复利 APY，也不代表已实现收益：

    年化（%） = 权利金 ÷（执行价 × 数量基数）× 365 ÷ 到期天数 × 100

该值由 Rust 使用 Binance 候选、成交或持仓数据计算，前端只负责展示。

## 开发

安装依赖并启动 Tauri：

    pnpm install
    pnpm tauri:dev

只检查前端布局时可运行 pnpm dev；普通浏览器缺少 Tauri runtime，不能执行业务命令。

当前 Tauri 打包目标为 Linux `.deb` 和 `.rpm`。

常规检查与构建：

    pnpm lint
    pnpm typecheck
    pnpm build
    cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
    cargo fmt --manifest-path src-tauri/Cargo.toml
    pnpm tauri:build

## 许可证

项目自有源码、文档和资源使用 GNU Affero General Public License v3.0，SPDX 标识为 AGPL-3.0-only。详见 [LICENSE](LICENSE)。
