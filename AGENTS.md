# AGENTS.md

## 协作规则

- 默认使用中文沟通。
- 不要把真实 API Secret、签名 query、订单响应原文写入日志或前端状态。
- 本项目只保留实盘路径；不要恢复模式切换、离线占位数据或浏览器数据 fallback。
- 任何下单功能都按真实下单处理，必须有显式确认门禁和审计事件。
- 不要用 `f64` 表示金额、价格、数量、权利金、手续费或收益率；Rust 侧使用 `rust_decimal::Decimal`，前端以字符串承接。

## 实盘边界

- 业务数据只允许来自 Binance 实盘接口或本地 SQLite 中已记录的真实事件。
- 普通浏览器运行仅用于检查布局；缺少 Tauri runtime 时应暴露明确错误，不要补前端数据 fallback。
- 实盘行情、账户、保证金、候选扫描未接入前，应返回空状态或明确错误。
- 历史订单只展示带交易所订单号的记录；不要展示旧的本地占位订单。

## 技术栈

- 前端：Vue 3、`<script setup lang="ts">`、Pinia、Naive UI、Vite。
- 桌面：Tauri 2。
- 后端：Rust 2024 edition、SQLite、reqwest、rust_decimal。
- 包管理：pnpm。

## Vue 约定

- route/view 级组件只做组合，不把跨页面状态写在局部组件里。
- 组件使用 props down / events up；全局状态放 Pinia。
- 复杂 UI 拆到 `src/components/common` 或对应 feature 组件。
- primitive 响应式状态优先使用 `shallowRef`；可整体替换的数据也优先按 immutable 方式更新。
- 不在 template 里写复杂计算，先用 `computed` 或 composable。

## Rust 约定

- 金融领域类型集中在 `src-tauri/src/domain.rs`。
- Binance 接口只放在 `binance.rs`，不要让前端或策略层拼签名。
- 策略计算放在 `strategy.rs`，状态编排放在 `app_state.rs`。
- 所有 command 返回 `Result<T, String>`，内部用 `AppError` 表达错误。
- 订单、审计、资金变更应写不可变事件；不要覆盖历史事实。

## Tauri 约定

- `src-tauri/icons/icon.png` 是 Linux 窗口和托盘启动路径的一部分，必须保持为有效 RGBA PNG。
- 修改图标资源时确认 `src-tauri/build.rs` 会触发 Tauri context 缓存重建，否则可能复用旧的坏图标缓存。
- 如果 `pnpm tauri:dev` 在 `gdk-pixbuf` 报 `data.len() must fit the width, height, and row_stride`，先检查图标 PNG 和缓存重建，而不是前端页面。

## 风控边界

- v1 只支持 `BTC/USDT`，新增标的必须补齐合约过滤器、保证金估算、风险规则和测试。
- 自动策略默认稳健：`0.01` 张、年化 `>= 15%`、`abs(delta) <= 0.30`、`IV Rank <= 60%`、到期 `7-45` 天。
- 实盘自动策略开启必须二次确认；之后自动单不逐笔确认。
- 到期 ITM 后现货补齐失败必须进入失败状态，停止相关自动动作并发出风险通知。

## 验证要求

- 前端改动至少运行 `pnpm typecheck`。
- Rust 改动至少运行 `cargo test` 或 `cargo check`。
- Tauri 启动路径、图标、窗口或托盘改动需要实际运行 `pnpm tauri:dev`。
- 修改交易、结算、风控逻辑时必须补单元测试。
- UI 改动需要检查 1180px 最小桌面宽度下没有文字重叠和表格溢出不可用问题。
