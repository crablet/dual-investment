# 双币赢助手

通过 Binance 期权与现货执行 BTC/USDT 双币赢式策略的 Tauri 实盘客户端。

## 使用

在“设置”连接 Binance，再到“双币赢”选择卖 Put 低买或卖 Call 高卖，复核后提交。已有未到期的 BTC/USDT 卖方义务仓可在“策略订单”接管。

启动时会尝试使用 keyring 中已保存的凭据连接。连接成功或点击“同步订单”会读取交易所状态；平时页面刷新读取本地快照，后台定期维护订单并刷新账户展示。账户查询失败时保留上次成功的数据，并显示错误提示。

已连接且进入“双币赢”时没有缓存候选，会自动扫描；已有候选通过“刷新候选”更新。修改数量时的权利金和保本价是本地预估，提交后以实际成交回报为准。

期权采用现金结算。保持客户端运行并连接，后台处理价内到期订单的现货补齐和资金归集。未完成批次会预留资金并限制新策略。

异常在“策略订单 → 恢复中心”处理；符合条件的失败收尾可选择“结束理财收尾”，未申购资金留在现货。保证金门槛、备资顺序和恢复条件见[实盘交易安全](docs/trading-safety.md)。

API Key 关闭提现权限，仅授予读取、现货与期权交易、Simple Earn 和 Universal Transfer 所需权限；凭据保存在系统 keyring。

## 资产与收益口径

同名资产合并展示并保留账户来源；合并后无法完整估值的资产保留数量，不计入总资产估值和比例图。

已成交订单的权利金按 `实际成交均价 × 已成交数量` 计算；保本价为卖 Put 的 `执行价 − 成交均价` 或卖 Call 的 `执行价 + 成交均价`。加载本地订单时会从成交字段重算这些派生指标，读取过程不改写数据库。

义务仓概览只统计运行中或部分成交且有实际成交的订单。卖 Put 的“已成交名义本金”为 `执行价 × 已成交数量`，实际保证金占用以 Binance 账户记录为准。

累计收益 = 订单权利金 + 卖 Put 补齐 BTC 的浮动盈亏，后者按 `(现价 − 补齐成交均价) × 补齐成交数量` 计算。不计手续费。

收益估值中的现价取最近一次成功账户刷新中的有效 BTC/USDT 价格；未取得有效价格时，浮动盈亏暂显示为 0。

日期展示和按日统计使用系统本地时区；“今日权利金”按订单创建日期汇总。到期判断使用合约精确时间。

年化为单利估算：

    年化（%） = 权利金 ÷（执行价 × 数量基数）× 365 ÷ 到期天数 × 100

候选使用预估权利金、候选数量和扫描时点；订单使用成交权利金、已成交数量和记录中的创建时点。到期天数向上取整，至少按 1 天计算。

## 开发

需要 Rust 1.95+ 和 pnpm：

    pnpm install
    pnpm tauri:dev

`pnpm dev` 仅用于浏览器布局检查，不提供业务能力。

TypeScript 暂保留在 6.0.x，以满足 [typescript-eslint 的版本要求](https://typescript-eslint.io/users/dependency-versions/)；升级到 7 前须确认 lint 与 Vue 类型检查工具链兼容。

维护时遵循 [AGENTS.md](AGENTS.md)，并保持以下边界：

- [AppState](src-tauri/src/app_state.rs) 编排交易、恢复和后台维护。`get_app_snapshot` 只读取本地状态和账户缓存；`sync_live_state` 完成同步后不返回快照，由 Pinia 再读取完整状态。
- [策略层](src-tauri/src/strategy.rs) 使用 Decimal 计算金额、数量和收益，前端以字符串接收。`preview_strategy` 只做本地预估；金融字段在前端的数值转换仅用于格式化和图形比例。
- [错误分类](src-tauri/src/errors.rs) 决定查询重试、提交结果是否待确认和连接重试资格。[IPC](src-tauri/src/commands.rs) 的业务错误返回 `{ message, canAutoReconnect }`，前端不根据文案判断重连；Tauri 自身的传输或参数错误不授予自动重连资格。

按改动范围验证：

- 前端：`pnpm lint`、`pnpm typecheck`；依赖、构建或工具配置变更加跑 `pnpm build`。
- Rust：先通过 `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`，再运行 `cargo fmt --manifest-path src-tauri/Cargo.toml`。
- UI：检查 1180px 最小宽度下的布局；Tauri 启动、图标或窗口变更须实际启动验证。

`pnpm tauri:build` 生成 Linux `.deb`、`.rpm`；加 `--no-bundle` 仅生成 `src-tauri/target/release/dual-investment` 可执行文件。

## 许可证

项目自有内容使用 [AGPL-3.0-only](LICENSE)。
