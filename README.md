# 双币赢助手

一个 Tauri 桌面客户端，用 Binance 期权 + 现货执行类似“双币赢”的实盘策略，目标是减少交易所打包产品对权利金收益的抽水。

Rust 后端要求 Rust 1.95 或更新版本。

## 功能范围

- 桌面端：Tauri 2 + Vue 3 + Naive UI + Rust。
- 交易范围：v1 仅支持 BTC/USDT；策略方向为卖 Put 低买、卖 Call 高卖，候选到期范围 1-90 天。
- 双币赢工作台：方向选择、候选筛选、按张数下单和自动复投设置在同一页面完成；手动发送订单前会进入应用内复核。
- 账户和资金：后端汇总期权保证金、现货账户和 Binance Simple Earn 真实持仓；资金不足时在受控流程内赎回活期或划转保证金。
- 订单和恢复：期权下单、现货补齐和资金动作都保留可恢复记录；请求结果不确定时重新查询 Binance 事实，未确认前阻断新的真实资金动作。
- 持仓接管：订单页可接管当前 Binance 期权义务仓，让本机继续同步、到期结算和必要的现货补齐。
- 运行观测：后台同步返回健康状态，设置页可查看脱敏日志；总览事件日历从当前快照派生。
- 凭据连接：API 凭据保存在系统 keyring。已保存凭据可自动重连，但权限错误、凭据缺失等人工错误会暂停自动重连。

## 风险边界

本应用只保留 Binance 实盘路径，会发起真实订单、Simple Earn 活期申购/赎回和现货-期权账户划转。API Key 必须关闭提现权限，只开启读取账户、读取行情、现货交易、期权交易、Simple Earn 和 Universal Transfer 所需权限。

Binance 期权是现金结算；到期 ITM 后，应用会通过额外的现货补齐订单形成类似双币赢的换币结果。期权到期时间使用 Binance exchangeInfo.expiryDate 派生的 expiryAt，到期结算价格只使用 Binance 行权记录接口。

真实下单和资金移动不会在网络失败时盲目重发。存在未确认的订单或资金恢复记录时，应用会阻断自动策略、手动下单和结算中的后续资金动作，直到重新查询 Binance 事实并落定状态。

下单前会校验合约状态、价格和数量过滤器、Binance 当前价格上下限、可用保证金和保证金占用率；默认保证金占用率风控上限为 60%。

业务数据只来自 Binance 接口或本地 SQLite 中已记录的真实事件。普通浏览器只适合检查布局，业务命令需要在 Tauri 客户端内执行。本地日志只写脱敏摘要，不能写 API Secret、签名 query 或 Binance 原始响应。

当前仍处于开发测试阶段，不维护旧 SQLite JSON payload、旧 IPC 或旧前端 DTO 兼容。结构变更涉及本地测试库时，使用一次性本地命令或临时脚本迁移/清理数据；命令或脚本不提交进代码库，也不放进应用启动路径。

## 开发运行

    pnpm install
    pnpm tauri:dev

检查前端布局时可以运行 pnpm dev，但普通浏览器没有 Tauri runtime，业务命令会返回“只能在 Tauri 客户端中执行”。

常规检查和构建：

    pnpm lint
    pnpm typecheck
    pnpm build
    cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
    cargo fmt --manifest-path src-tauri/Cargo.toml
    pnpm tauri:build

需要前端自动修复时运行 pnpm lint:fix，再复查 diff。

默认 Linux 打包目标是 deb 和 rpm，产物位于 src-tauri/target/release/bundle/。AppImage 当前不作为默认目标。

## 项目结构

    src/
      components/   Vue 页面和通用组件
      composables/  可复用前端逻辑
      services/     Tauri API 调用
      stores/       Pinia 应用状态和展示派生 helper
      theme/        Naive UI 全局主题覆盖
      types/        前后端共享数据形状
      utils/        前端纯展示工具

    src-tauri/
      src/app_state.rs 与 src/app_state/   应用状态、订单、资金和恢复编排
      src/binance.rs 与 src/binance/       Binance 请求、签名和响应解析
      src/domain.rs                       金融领域模型
      src/persistence.rs                  SQLite 持久化
      src/secrets.rs                      系统 keyring 凭据读写
      src/logs.rs                         每日脱敏日志
      src/strategy.rs 与 src/strategy/     策略计算、候选、风险和 Decimal 工具
