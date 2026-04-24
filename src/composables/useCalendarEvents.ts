// 事件日历派生逻辑。
//
// 日历只从当前应用快照派生订单、到期、结算、补齐、自动复投、风险和恢复事件。

import { computed, toValue, type MaybeRefOrGetter } from 'vue'
import { useAppStore, type AppViewId } from '@/stores/app'
import type { OrderAttemptView, RiskAlert, StrategyOrder } from '@/types/domain'
import {
  addDays,
  formatDateKey,
  localDateKey,
  localTimeLabel,
  parseDateKey,
  scheduleTimeLabel,
  startOfDay,
} from '@/utils/date'
import { attemptStatusText, strategyStatusText } from '@/utils/labels'

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

/**
 * 根据可见日期范围派生日历事件。
 *
 * 返回值是 computed，组件只负责筛选和展示，不维护独立日程数据。
 */
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

    return rows
      .filter(event => isDateInRange(event.date, start, end))
      .sort(compareEvents)
  })

  return {
    events,
  }
}

/** 从策略订单派生下单、到期、结算和现货补齐事件。 */
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
      detail: `${order.optionSymbol}，执行价 ${order.strike}，状态 ${strategyStatusText(order.status)}`,
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

/** 从风控提醒派生日历事件。 */
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

/** 从恢复记录派生日历事件；已持久化成功的记录不再提示待办。 */
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

/** 按当前自动规则生成可见日期范围内的计划扫描事件。 */
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

/** 同日事件按优先级高的先展示。 */
function compareEvents(left: CalendarEvent, right: CalendarEvent) {
  const dateCompare = left.date.localeCompare(right.date)
  if (dateCompare !== 0) return dateCompare
  if (left.priority !== right.priority) return right.priority - left.priority
  return (left.timeLabel ?? '').localeCompare(right.timeLabel ?? '')
}

/** 判断日期键是否位于可见范围内。 */
function isDateInRange(dateKey: string, start: Date, end: Date) {
  const date = parseDateKey(dateKey)
  return date >= start && date <= end
}

/** 判断 Decimal 字符串是否有非零值。 */
function decimalStringHasValue(value: string) {
  return !/^0(?:\.0+)?$/.test(value)
}
