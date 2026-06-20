# AGENTS.md

默认使用中文，按任务阅读相关文档。修改实盘连接、下单、资金或恢复逻辑前，先读[实盘交易安全](docs/trading-safety.md)；检查按[开发指南](docs/development.md#验证与打包)执行。

## 项目约束

- 仅 Binance 实盘；无 Tauri runtime 时业务命令报错。新策略仅支持 BTC/USDT，由用户在“双币赢”页手动确认。
- 不增加托盘等常驻入口或持仓风险提醒；保留交易保护检查和错误反馈。
- 金融计算与交易编排在 Rust，使用 Decimal，禁用浮点；前端以字符串接收快照，Pinia 管理共享状态。
- 金额、持仓和交易结果以 Binance 或 SQLite 记录为准，不能根据日志和前端进度推断。不引入手续费解析、计算或账务。
- 凭据仅经 `src-tauri/src/secrets.rs` 写入系统 keyring；密钥、签名请求及 Binance 原始响应不得进入日志、SQLite、前端或导出文件。
- 开发期不保留旧 IPC、DTO、SQLite 兼容或迁移分支；契约变更同步 Rust、TypeScript 和 Pinia。
- 文案统一经 `src/i18n.ts` 与 `src/locales/` 管理，全部语言同步消息键与插值参数；词典按需加载，语言偏好独立于交易状态。
- 业务消息以 `{ code, params }` 跨 IPC、SQLite 和日志保存，展示时翻译；不得根据翻译文案判断交易、恢复或重试。
- 不保留测试、夹具或快照；交付前清理临时验证代码、依赖和配置。
