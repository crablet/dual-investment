# AGENTS.md

## 基线

- 默认使用中文沟通。
- 仅保留 Binance 实盘业务；缺少 Tauri runtime 时业务命令必须报错，浏览器仅用于布局检查。
- 新期权策略仅支持 BTC/USDT，由用户在“双币赢”页手动确认发起；不增加托盘等常驻入口。
- 金融事实只来自 Binance 或 SQLite 的真实记录，不以日志或前端进度作为事实依据。
- 金融数值禁用浮点；Rust 使用 rust_decimal::Decimal，前端以字符串承接。
- 收益模型不计手续费，不新增手续费解析或账务。
- 凭据仅经 src-tauri/src/secrets.rs 写入系统 keyring；密钥、签名请求及 Binance 原始响应不得进入日志、SQLite、前端或导出文件。
- 项目自有内容使用 AGPL-3.0-only。
- 开发期不保留旧 IPC、DTO、SQLite 的兼容或迁移分支；结构变更同步更新 Rust、TypeScript 和 Pinia。
- 仓库不保留测试、夹具或快照；交付前清理临时验证代码、依赖和配置。

## 实现边界

- 金融计算与交易编排留在 Rust，前端展示后端快照，Pinia 管理共享状态；模块按职责划分。
- 修改实盘连接、下单、资金或恢复逻辑前，阅读 [实盘交易安全](docs/trading-safety.md)。

## 验证

命令统一见 [README 开发说明](README.md#开发)。

- 前端改动执行 lint 和类型检查；依赖、构建或工具配置变化时补做前端构建。
- Rust 改动先通过 Clippy，再格式化。
- Tauri 启动、图标或窗口改动实际启动验证；UI 检查 1180px 最小宽度下无文字重叠或不可用溢出。
