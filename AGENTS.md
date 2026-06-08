# AGENTS.md

默认使用中文。修改实盘连接、下单、资金或恢复逻辑前，阅读[实盘交易安全](docs/trading-safety.md)。

## 项目约束

- 仅 Binance 实盘；无 Tauri runtime 时业务命令报错。新策略仅支持 BTC/USDT，由用户在“双币赢”页手动确认。
- 不增加托盘等常驻入口或持仓风险提醒；保留交易保护检查和错误反馈。
- 金融计算与交易编排在 Rust，使用 Decimal；前端以字符串接收快照，Pinia 管理共享状态。金融数值禁用浮点。
- 金融事实来自 Binance 或 SQLite 真实记录，日志和前端进度不能作为事实依据。
- 不引入手续费解析、计算或账务。
- 凭据仅经 src-tauri/src/secrets.rs 写入系统 keyring；密钥、签名请求及 Binance 原始响应不得进入日志、SQLite、前端或导出文件。
- 开发期不保留旧 IPC、DTO、SQLite 兼容或迁移分支；契约变更同步 Rust、TypeScript 和 Pinia。
- 不保留测试、夹具或快照；交付前清理临时验证代码及其依赖、配置。

## 验证

按 [README 开发说明](README.md#开发)执行与改动相关的检查。
