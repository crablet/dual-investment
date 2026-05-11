# 实盘交易安全

本文只在修改连接、后台同步、订单、资金、恢复、持仓接管或到期结算时阅读。它记录不能从单个函数局部判断的跨流程约束。

## 候选与下单

- 候选期限为 1–90 天。低买只保留执行价低于 BTCUSDT 现货价的 Put，高卖只保留执行价高于现货价的 Call。
- 订单和请求携带 Binance exchangeInfo.expiryDate 派生的精确 expiryAt；结算价格只来自 Binance 行权记录，不能用现货价代替。
- 价格和数量必须满足 PRICE_FILTER.tickSize 与 LOT_SIZE.stepSize。
- 候选可用 24h ticker 买一价估算。真实卖出 IOC 前必须重新读取订单簿买一价和买一量；买一量不足时阻断，不下探深度，也不用 mark price、现货价或前端缓存价替代。
- 默认保证金占用上限为 60%；最终发送允许 5 个百分点的快照容差，预计不高于 65% 才能通过。容差不能用于减少补资或增加闲置资金归集。
- 复核弹窗必须展示冻结快照中的方向、合约、价格、数量、到期、预计收益和风险。

## 资金与结算

- 资金准备只能依据现货、期权余额和 Simple Earn 真实持仓。
- Simple Earn 与 Universal Transfer 必须先写 funding_attempts，再请求 Binance。
- 请求确认后必须等待账户快照反映到账；等待期间只查询，不重复发送。OPTION_MAIN 到账后才能申购 Simple Earn。
- 到期 USDT 归集是结算批次级收尾任务，默认延迟约 5 分钟。低于 10 USDT 不归集；存在待恢复记录、未完成结算或未完成补齐时延后。
- 同一 chain_id 已确认的划转或申购只能续跑下一步，不得重发已经确认的动作。

## 恢复与接管

- 下单或现货补齐结果不确定时，只能按 clientOrderId 或交易所订单号查询确认，不能盲目重发；order_attempts 保留到状态落定。
- 期权订单被 Binance 确认为 EXPIRED、REJECTED、CANCELED 或 CANCELLED 且成交量为零时，必须标记失败，不能创建或继续跟踪为活跃订单。
- 资金结果不确定时，只能按交易所引用，或同类型、同资产、同金额的唯一历史记录确认；匹配不唯一时进入 blocked。
- 查询 Binance 失败属于临时错误，不增加 effective_query_count；只有历史接口成功返回但未找到记录时才增加。恢复记录只保存脱敏诊断。
- blocked 记录只能在人工核对 Binance 后改为 confirmed 或 failed；解除阻断不得重新发送资金请求。
- 持仓接管只从当前义务仓和 exchangeInfo 创建 source=adoptedPosition 的本地订单。接管入口不下单、不移动资金，也不把接管状态混入恢复记录。
