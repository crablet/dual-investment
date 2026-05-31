# 双币赢助手

通过 Binance 期权与现货执行 BTC/USDT 双币赢式策略的 Tauri 实盘桌面客户端。

## 使用

在“双币赢”页选择卖 Put 低买或卖 Call 高卖候选，复核后手动提交；也可接管已有 Binance 期权义务仓。

Binance 期权采用现金结算。保持客户端运行并连接，应用等待同批行权记录齐全后整批备资、逐笔为价内订单补齐现货，完成后延迟归集。重连后依据持久化进度继续处理；未完成的到期批次（含理财收尾）会预留资金并限制新策略。

异常在“策略订单 → 恢复中心”处理。仅理财收尾失败且满足[人工结束条件](docs/trading-safety.md#资金与恢复)时，可选择“结束理财收尾”：未申购资金留在现货，停止本批次后续收尾并解除其预留；其他批次及新订单的资金、风控检查仍然有效。

应用会执行真实期权和现货交易、Simple Earn 申购赎回及账户划转。API Key 关闭提现权限，仅授予账户和行情读取、现货与期权交易、Simple Earn、Universal Transfer 所需权限。

## 资产与收益口径

资产页可展示其他币种，同名资产合并并保留账户来源；同轮快照中精确匹配的 Simple Earn 镜像会去重。缺少有效价格的资产保留数量，不计入估值合计。

累计收益为订单权利金，加上卖 Put 现货补齐记录按 `(现价 − 补齐成交均价) × 补齐成交数量` 计算的浮动盈亏。当前模型未计期权和现货手续费。

年化收益率为单利估算，不代表复利 APY 或已实现收益：

    年化（%） = 权利金 ÷（执行价 × 数量基数）× 365 ÷ 到期天数 × 100

## 开发

需要 Rust 1.95 或更新版本。安装依赖并启动：

    pnpm install
    pnpm tauri:dev

仅检查布局可运行 pnpm dev；普通浏览器不能执行业务命令。

检查与构建命令：

    pnpm lint
    pnpm typecheck
    pnpm build
    cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
    cargo fmt --manifest-path src-tauri/Cargo.toml
    pnpm tauri:build

当前打包目标为 Linux `.deb` 和 `.rpm`。若只需本地 release 可执行文件，运行 `pnpm tauri:build --no-bundle`，产物位于 `src-tauri/target/release/dual-investment`；重启新版程序后生效。交易逻辑约束见 [实盘交易安全](docs/trading-safety.md)。

## 许可证

项目自有内容使用 AGPL-3.0-only，详见 [LICENSE](LICENSE)。
