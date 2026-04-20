# AGENTS.md

## 协作规则

- 默认使用中文沟通。
- 不要把真实 API Secret、签名 query、订单响应原文写入日志或前端状态。
- API Key/Secret 只允许通过 `src-tauri/src/secrets.rs` 写入系统 keyring；不要保存到 SQLite、审计日志、前端 store 或导出文件。
- 本项目只保留实盘路径；不要恢复模式切换、离线占位数据或浏览器数据 fallback。
- 任何下单功能都按真实下单处理，必须有显式确认门禁和审计事件。
- 不要用 `f64` 表示金额、价格、数量、权利金、手续费或收益率；Rust 侧使用 `rust_decimal::Decimal`，前端以字符串承接。

## 实盘边界

- 业务数据只允许来自 Binance 实盘接口或本地 SQLite 中已记录的真实事件。
- 普通浏览器运行仅用于检查布局；缺少 Tauri runtime 时应暴露明确错误，不要补前端数据 fallback。
- 实盘行情、账户、保证金、候选扫描未接入前，应返回空状态或明确错误。
- 已保存凭据可在应用启动时自动读取并连接 Binance，但不得自动开启自动策略。
- 候选扫描结果可以保存在运行态内存快照中；普通账户/订单同步不应把最近一次真实扫描结果清空。
- 历史订单只展示带交易所订单号的记录；不要展示旧的本地占位订单。
- 交易所已接单但本地订单保存失败时，必须保留 `order_attempts` 恢复记录和明确错误提示；不要吞掉交易所订单号。
- 恢复 `order_attempts` 时必须重新查询 Binance 真实订单；现货补齐恢复必须能匹配已有本地期权订单，不要凭恢复记录凭空创建补齐后的期权订单。

## 技术栈

- 前端：Vue 3、`<script setup lang="ts">`、Pinia、Naive UI、Vite。
- 桌面：Tauri 2。
- 后端：Rust 2024 edition、SQLite、reqwest、rust_decimal、keyring-core + 平台 keyring store。
- 包管理：pnpm。

## Vue 约定

- route/view 级组件只做组合，不把跨页面状态写在局部组件里。
- 组件使用 props down / events up；全局状态放 Pinia。
- 复杂 UI 拆到 `src/components/common` 或对应 feature 组件。
- primitive 响应式状态优先使用 `shallowRef`；可整体替换的数据也优先按 immutable 方式更新。
- 不在 template 里写复杂计算，先用 `computed` 或 composable。
- 双币赢和自动复投共用 `src/components/views/DualInvestmentView.vue`；不要恢复 `auto` 视图 id、独立 `AutoInvestView.vue` 或单独导航入口。
- 双币赢下单数量按 Binance 期权“张”计算，不要改成按 USDT 金额申购。
- 候选表和下单表单中的期权权利金单价统一写成“价格”，不要改回“单位限价”；候选表不要展示数量列或“低买 BTC / 高卖 BTC”副文案。
- 候选表筛选控件放在表格上方，表头只保留字段名；筛选应作用于前端已收到的全部候选，不要为了 UI 简化恢复后端固定截断。
- 进入双币赢工作台时应在已连接且空闲的情况下自动扫描一次候选；这只是拉取候选，不得自动开启自动策略或自动下单。
- 指标卡、摘要卡和列表项避免“本地记录”“已记录订单”“按当前价格估算”等解释性副文案；上下文足够时只保留字段名和数值，必要说明放到详情、tooltip 或 `aria-label`。
- 账户、持仓、义务仓这类多列列表必须使用稳定 grid 列宽；标签列左对齐，数字列右对齐并使用等宽数字。余额和持仓数量在列表中默认显示两位小数，交易、审计、下单参数仍按原始字符串精度处理。

## Rust 约定

- 金融领域类型集中在 `src-tauri/src/domain.rs`。
- Binance 接口只放在 `binance.rs`，不要让前端或策略层拼签名。
- 系统凭据读写只放在 `secrets.rs`，通过 `keyring-core` 显式配置平台 store；调用方只接收连接状态或脱敏 API Key。
- 策略计算放在 `strategy.rs`，状态编排放在 `app_state.rs`。
- 候选方向必须用 BTCUSDT 现货价过滤：低买只保留执行价低于现货的 Put，高卖只保留执行价高于现货的 Call。
- 候选浏览范围保持为到期 `1-90` 天；自动复投规则只影响推荐评级，不要用自动规则的到期天数、delta、IV、年化去隐藏候选。
- 候选扫描不要丢弃符合浏览范围的候选；如需性能优化，应优先做前端筛选、排序或虚拟列表，不要在后端固定截断。
- 候选扫描应返回诊断统计，至少包含原始合约数、BTC 合约数、方向命中数、到期命中数和最终返回数。
- 所有 command 返回 `Result<T, String>`，内部用 `AppError` 表达错误。
- 订单、审计、资金变更应写不可变事件；不要覆盖历史事实。

## Tauri 约定

- `src-tauri/icons/icon.png` 是 Linux 窗口和托盘启动路径的一部分，必须保持为有效 RGBA PNG。
- 修改图标资源时确认 `src-tauri/build.rs` 会触发 Tauri context 缓存重建，否则可能复用旧的坏图标缓存。
- 默认 Linux bundle 目标保持为 `deb` 和 `rpm`；不要随手改回 `all`，当前 Tauri 使用的 `linuxdeploy` AppImage 路径在较新系统库上会因 `.relr.dyn` strip 兼容性失败。
- 如果 `pnpm tauri:dev` 在 `gdk-pixbuf` 报 `data.len() must fit the width, height, and row_stride`，先检查图标 PNG 和缓存重建，而不是前端页面。

## 风控边界

- v1 只支持 `BTC/USDT`，新增标的必须补齐合约过滤器、保证金估算、风险规则和测试。
- 自动策略默认稳健：`0.01` 张、年化 `>= 15%`、`abs(delta) <= 0.30`、`IV Rank <= 60%`、到期 `7-45` 天。
- 自动策略规则可以持久化，但应用启动后自动执行必须保持关闭，重新开启仍需二次确认。
- 自动策略开启必须二次确认；之后自动单不逐笔确认。
- 到期 ITM 后现货补齐失败必须进入失败状态，停止相关自动动作并发出风险通知。

## 验证要求

- 前端改动至少运行 `pnpm typecheck`。
- Rust 改动至少运行 `cargo test` 或 `cargo check`。
- Tauri 启动路径、图标、窗口或托盘改动需要实际运行 `pnpm tauri:dev`。
- 修改交易、结算、风控逻辑时必须补单元测试。
- UI 改动需要检查 1180px 最小桌面宽度下没有文字重叠和表格溢出不可用问题。
