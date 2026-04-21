import { computed, toValue, type MaybeRefOrGetter } from 'vue'
import { useAppStore, type AppViewId } from '@/stores/app'
import type { AuditEvent, OrderAttemptView, RiskAlert, StrategyOrder } from '@/types/domain'

export type CalendarEventType =
  | 'order'
  | 'expiry'
  | 'settlement'
  | 'rebalance'
  | 'auto'
  | 'risk'
  | 'recovery'
  | 'system'

export type CalendarEventTone = 'gold' | 'green' | 'blue' | 'purple' | 'red' | 'gray'

export interface CalendarEvent {
  id: string
  type: CalendarEventType
  date: string
  timeLabel?: string
  title: string
  detail: string
  tone: CalendarEventTone
  targetView: AppViewId
  priority: number
}

export interface CalendarEventTypeMeta {
  id: CalendarEventType | 'all'
  label: string
  tone: CalendarEventTone
}

interface UseCalendarEventsOptions {
  startDate: MaybeRefOrGetter<Date>
  endDate: MaybeRefOrGetter<Date>
}

export const calendarEventTypes: CalendarEventTypeMeta[] = [
  { id: 'all', label: '全部', tone: 'gold' },
  { id: 'order', label: '买入', tone: 'gold' },
  { id: 'expiry', label: '到期', tone: 'purple' },
  { id: 'settlement', label: '结算', tone: 'green' },
  { id: 'rebalance', label: '补齐', tone: 'green' },
  { id: 'auto', label: '自动复投', tone: 'blue' },
  { id: 'risk', label: '风险', tone: 'red' },
  { id: 'recovery', label: '恢复', tone: 'gray' },
  { id: 'system', label: '系统', tone: 'gray' },
]

export function useCalendarEvents(options: UseCalendarEventsOptions) {
  const store = useAppStore()

  const events = computed(() => {
    const start = startOfDay(toValue(options.startDate))
    const end = startOfDay(toValue(options.endDate))
    const rows: CalendarEvent[] = []

    for (const order of store.orders) {
      rows.push(...eventsFromOrder(order))
    }

    if (store.settings?.autoEnabled && store.autoRule?.enabled) {
      rows.push(...autoEventsBetween(start, end, store.autoRule.schedule, store.autoRule.quantity, store.autoRule.minAnnualized))
    }

    for (const alert of store.risk?.alerts ?? []) {
      rows.push(eventFromRiskAlert(alert))
    }

    for (const attempt of store.orderAttempts) {
      const event = eventFromOrderAttempt(attempt)
      if (event) {
        rows.push(event)
      }
    }

    for (const audit of store.auditRecent) {
      const event = eventFromAudit(audit)
      if (event) {
        rows.push(event)
      }
    }

    return rows
      .filter(event => isDateInRange(event.date, start, end))
      .sort(compareEvents)
  })

  return {
    events,
  }
}

function eventsFromOrder(order: StrategyOrder): CalendarEvent[] {
  const direction = order.direction === 'sellPut' ? '低买 BTC' : '高卖 BTC'
  const rows: CalendarEvent[] = [
    {
      id: `order-${order.id}`,
      type: 'order',
      date: localDateKey(order.createdAt),
      timeLabel: localTimeLabel(order.createdAt),
      title: `${direction} 下单`,
      detail: `${order.optionSymbol}，${order.quantity} 张，权利金 ${order.premium} USDT`,
      tone: 'gold',
      targetView: 'orders',
      priority: 30,
    },
    {
      id: `expiry-${order.id}`,
      type: 'expiry',
      date: order.expiry,
      title: `${direction} 到期日`,
      detail: `${order.optionSymbol}，执行价 ${order.strike}，状态 ${orderStatusText(order.status)}`,
      tone: 'purple',
      targetView: 'orders',
      priority: 70,
    },
  ]

  if (order.settledAt) {
    rows.push({
      id: `settlement-${order.id}`,
      type: 'settlement',
      date: localDateKey(order.settledAt),
      timeLabel: localTimeLabel(order.settledAt),
      title: '订单结算完成',
      detail: order.rebalanceMessage || `${order.optionSymbol} 已完成到期结算`,
      tone: 'green',
      targetView: 'orders',
      priority: 60,
    })
  }

  if (hasRebalanceEvent(order)) {
    rows.push({
      id: `rebalance-${order.id}`,
      type: 'rebalance',
      date: localDateKey(order.settledAt ?? order.updatedAt),
      timeLabel: localTimeLabel(order.settledAt ?? order.updatedAt),
      title: order.status === 'rebalanceFailed' ? '现货补齐失败' : '现货补齐',
      detail: order.rebalanceMessage || `${order.optionSymbol} 现货补齐状态 ${order.spotOrderStatus || '--'}`,
      tone: order.status === 'rebalanceFailed' ? 'red' : 'green',
      targetView: order.status === 'rebalanceFailed' ? 'risk' : 'portfolio',
      priority: order.status === 'rebalanceFailed' ? 90 : 65,
    })
  }

  return rows
}

function eventFromRiskAlert(alert: RiskAlert): CalendarEvent {
  return {
    id: `risk-${alert.id}`,
    type: 'risk',
    date: localDateKey(alert.createdAt),
    timeLabel: localTimeLabel(alert.createdAt),
    title: alert.title,
    detail: alert.message,
    tone: alert.severity === 'high' || alert.severity === 'medium' ? 'red' : 'gold',
    targetView: 'risk',
    priority: alert.severity === 'high' ? 95 : 80,
  }
}

function eventFromOrderAttempt(attempt: OrderAttemptView): CalendarEvent | undefined {
  if (attempt.status === 'persisted' || attempt.status === 'spot_persisted') {
    return undefined
  }

  return {
    id: `recovery-${attempt.id}`,
    type: 'recovery',
    date: localDateKey(attempt.updatedAt),
    timeLabel: localTimeLabel(attempt.updatedAt),
    title: attempt.status.includes('failed') ? '订单恢复待办' : '订单尝试记录',
    detail: `${attempt.optionSymbol}，${attemptStatusText(attempt.status)}，${attempt.message}`,
    tone: attempt.status.includes('failed') ? 'red' : 'gray',
    targetView: 'orders',
    priority: attempt.status.includes('failed') ? 92 : 52,
  }
}

function eventFromAudit(audit: AuditEvent): CalendarEvent | undefined {
  const base = {
    date: localDateKey(audit.createdAt),
    timeLabel: localTimeLabel(audit.createdAt),
  }

  if (audit.kind === 'strategy.auto.no_candidate') {
    return {
      ...base,
      id: `audit-${audit.id}`,
      type: 'auto',
      title: '自动复投扫描',
      detail: '本次自动扫描无推荐候选',
      tone: 'blue',
      targetView: 'dual',
      priority: 42,
    }
  }

  if (audit.kind === 'strategy.rule.saved') {
    return {
      ...base,
      id: `audit-${audit.id}`,
      type: 'auto',
      title: '自动复投规则保存',
      detail: '自动复投筛选和下单规则已更新',
      tone: 'blue',
      targetView: 'dual',
      priority: 38,
    }
  }

  if (audit.kind === 'strategy.auto.toggled') {
    return {
      ...base,
      id: `audit-${audit.id}`,
      type: 'auto',
      title: '自动复投状态变更',
      detail: audit.message.includes('启用') ? '自动复投已启用' : '自动复投已暂停',
      tone: 'blue',
      targetView: 'dual',
      priority: 55,
    }
  }

  if (audit.kind === 'background.tick_failed' || audit.kind === 'strategy.order.sync_failed') {
    return {
      ...base,
      id: `audit-${audit.id}`,
      type: 'system',
      title: audit.kind === 'background.tick_failed' ? '后台同步失败' : '订单同步失败',
      detail: '需要检查连接状态或订单同步结果',
      tone: 'red',
      targetView: 'risk',
      priority: 88,
    }
  }

  return undefined
}

function autoEventsBetween(
  start: Date,
  end: Date,
  schedule: string,
  quantity: string,
  minAnnualized: string,
): CalendarEvent[] {
  const rows: CalendarEvent[] = []
  const timeLabel = scheduleTimeLabel(schedule)

  for (let day = new Date(start); day <= end; day = addDays(day, 1)) {
    const date = formatDateKey(day)
    rows.push({
      id: `auto-${date}`,
      type: 'auto',
      date,
      timeLabel,
      title: '自动复投扫描',
      detail: `${quantity} 张，目标年化不低于 ${minAnnualized}%`,
      tone: 'blue',
      targetView: 'dual',
      priority: 45,
    })
  }

  return rows
}

function hasRebalanceEvent(order: StrategyOrder) {
  return order.status === 'rebalanceFailed'
    || Boolean(order.spotOrderId)
    || Boolean(order.spotOrderStatus)
    || Boolean(order.spotClientOrderId)
    || decimalStringHasValue(order.spotFilledQuantity)
}

function compareEvents(left: CalendarEvent, right: CalendarEvent) {
  const dateCompare = left.date.localeCompare(right.date)
  if (dateCompare !== 0) return dateCompare
  if (left.priority !== right.priority) return right.priority - left.priority
  return (left.timeLabel ?? '').localeCompare(right.timeLabel ?? '')
}

function isDateInRange(dateKey: string, start: Date, end: Date) {
  const date = parseDateKey(dateKey)
  return date >= start && date <= end
}

function localDateKey(value: string) {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) {
    return value.slice(0, 10)
  }
  return formatDateKey(date)
}

function localTimeLabel(value: string) {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) {
    return undefined
  }
  return date.toLocaleTimeString('zh-CN', {
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
  })
}

function scheduleTimeLabel(schedule: string) {
  const matched = schedule.match(/([01]?\d|2[0-3]):([0-5]\d)/)
  if (!matched) {
    return '计划'
  }
  return `${matched[1].padStart(2, '0')}:${matched[2]}`
}

function formatDateKey(date: Date) {
  const year = date.getFullYear()
  const month = `${date.getMonth() + 1}`.padStart(2, '0')
  const day = `${date.getDate()}`.padStart(2, '0')
  return `${year}-${month}-${day}`
}

function parseDateKey(dateKey: string) {
  const [year, month, day] = dateKey.split('-').map(Number)
  return new Date(year, month - 1, day)
}

function startOfDay(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate())
}

function addDays(date: Date, days: number) {
  const next = new Date(date)
  next.setDate(next.getDate() + days)
  return next
}

function orderStatusText(status: StrategyOrder['status']) {
  const statusMap: Record<StrategyOrder['status'], string> = {
    pending: '待成交',
    running: '进行中',
    settled: '已结算',
    cancelled: '已取消',
    rejected: '已拒绝',
    expired: '已过期',
    partiallyFilled: '部分成交',
    rebalanceFailed: '补齐失败',
  }
  return statusMap[status]
}

function attemptStatusText(status: string) {
  const statusMap: Record<string, string> = {
    attempted: '准备发送期权订单',
    exchange_ack: '交易所已接单',
    persist_failed: '订单保存失败',
    spot_attempted: '准备发送现货补齐',
    spot_exchange_ack: '现货交易所已接单',
    spot_persist_failed: '现货补齐保存失败',
  }
  return statusMap[status] ?? status
}

function decimalStringHasValue(value: string) {
  return !/^0(?:\.0+)?$/.test(value)
}
