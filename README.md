# 双币赢助手

通过 Binance 期权与现货执行 BTC/USDT 双币赢式策略的 Tauri 实盘客户端。

## 使用

在“设置”连接 Binance，再到“双币赢”选择卖 Put 低买或卖 Call 高卖，复核后提交。已有未到期的 BTC/USDT 卖方义务仓可在“策略订单”接管。

期权采用现金结算。保持客户端运行并连接，后台处理价内到期订单的现货补齐和资金归集。未完成批次会预留资金并限制新策略。

异常在“策略订单 → 恢复中心”处理；符合条件的失败收尾可选择“结束理财收尾”，未申购资金留在现货。保证金门槛、备资顺序和恢复条件见[实盘交易安全](docs/trading-safety.md)。

API Key 关闭提现权限，仅授予读取、现货与期权交易、Simple Earn 和 Universal Transfer 所需权限；凭据保存在系统 keyring。

## 资产与收益口径

同名资产合并展示并保留账户来源；无有效价格的资产保留数量，不计入估值。

累计收益 = 订单权利金 + 卖 Put 补齐 BTC 的浮动盈亏，后者按 `(现价 − 补齐成交均价) × 补齐成交数量` 计算。不计手续费。

日期展示和按日统计使用系统本地时区；“今日权利金”按订单创建日期汇总。到期判断使用合约精确时间。

年化为单利估算：

    年化（%） = 权利金 ÷（执行价 × 数量基数）× 365 ÷ 到期天数 × 100

## 开发

需要 Rust 1.95+ 和 pnpm：

    pnpm install
    pnpm tauri:dev

`pnpm dev` 仅用于浏览器布局检查，不提供业务能力。

按改动范围验证：

- 前端：`pnpm lint`、`pnpm typecheck`；依赖、构建或工具配置变更加跑 `pnpm build`。
- Rust：先通过 `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`，再运行 `cargo fmt --manifest-path src-tauri/Cargo.toml`。
- UI：检查 1180px 最小宽度下的布局；Tauri 启动、图标或窗口变更须实际启动验证。

`pnpm tauri:build` 生成 Linux `.deb`、`.rpm`；加 `--no-bundle` 仅生成 `src-tauri/target/release/dual-investment` 可执行文件。

## 许可证

项目自有内容使用 [AGPL-3.0-only](LICENSE)。
