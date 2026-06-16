// 事件日历派生逻辑。
//
// 日历只从当前应用快照派生订单、到期、结算、补齐和恢复事件。

import type { MaybeRefOrGetter } from 'vue'
import type { AppViewId } from '@/stores/app'
import type { FundingAttemptView, OrderAttempt, StrategyOrder } from '@/types/domain'
import { computed, toValue } from 'vue'
import { messageText, t } from '@/i18n'
import { useAppStore } from '@/stores/app'
import {
  localDateKey,
  localTimeLabel,
  parseDateKey,
  startOfDay,
} from '@/utils/date'
import { decimalStringHasValue } from '@/utils/decimal'
import { attemptStatusText, fundingAttemptStatusText, strategyStatusText } from '@/utils/labels'

/** 日历事件类型只用于筛选和跳转，不对应独立持久化任务。 */
export type CalendarEventType
  = | 'order'
    | 'expiry'
    | 'settlement'
    | 'rebalance'
    | 'recovery'

/** 日历角标颜色语义，保持和全局交易终端配色一致。 */
export type CalendarEventTone = 'gold' | 'green' | 'purple' | 'red' | 'gray'

/** 从当前快照派生出的单条日历事件。 */
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

/** 日历顶部筛选项；顺序即 UI 展示顺序。 */
export const calendarEventTypes: CalendarEventTypeMeta[] = [
  { id: 'all', get label() { return t('ui.all') }, tone: 'gold' },
  { id: 'order', get label() { return t('ui.placeOrder') }, tone: 'gold' },
  { id: 'expiry', get label() { return t('ui.expiry') }, tone: 'purple' },
  { id: 'settlement', get label() { return t('ui.settlement') }, tone: 'green' },
  { id: 'rebalance', get label() { return t('ui.rebalancing') }, tone: 'green' },
  { id: 'recovery', get label() { return t('ui.recovery') }, tone: 'gray' },
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

    // 日历事件是快照的展示索引，不持久化；刷新后会重新从订单/恢复记录生成。
    for (const order of store.orders) {
      rows.push(...eventsFromOrder(order))
    }

    for (const attempt of store.orderAttempts) {
      const event = eventFromOrderAttempt(attempt)
      if (event) {
        rows.push(event)
      }
    }

    for (const attempt of store.fundingAttempts) {
      const event = eventFromFundingAttempt(attempt)
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
  const direction = order.direction === 'sellPut' ? t('ui.buyBtcLow') : t('ui.sellBtcHigh2')
  const rows: CalendarEvent[] = [
    {
      id: `order-${order.id}`,
      type: 'order',
      date: localDateKey(order.createdAt),
      timeLabel: localTimeLabel(order.createdAt),
      title: t('ui.order', { p0: direction }),
      detail: t('ui.filledContractsPremiumUsdt', { p0: order.optionSymbol, p1: order.filledQuantity, p2: order.premium }),
      tone: 'gold',
      targetView: 'orders',
      priority: 30,
    },
    {
      id: `expiry-${order.id}`,
      type: 'expiry',
      date: localDateKey(order.expiryAt),
      timeLabel: localTimeLabel(order.expiryAt),
      title: t('ui.expiry2', { p0: direction }),
      detail: t('ui.strikeStatus', { p0: order.optionSymbol, p1: order.strike, p2: strategyStatusText(order.status) }),
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
      get title() { return t('ui.orderSettled') },
      detail: messageText(order.rebalanceMessage) || t('ui.hasCompletedExpirySettlement', { p0: order.optionSymbol }),
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
      title: order.status === 'rebalanceFailed' ? t('ui.spotRebalancingFailed') : t('ui.spotRebalancing'),
      detail: messageText(order.rebalanceMessage) || t('ui.spotRebalancingStatus', { p0: order.optionSymbol, p1: order.spotOrderStatus || '--' }),
      tone: order.status === 'rebalanceFailed' ? 'red' : 'green',
      targetView: order.status === 'rebalanceFailed' ? 'orders' : 'portfolio',
      priority: order.status === 'rebalanceFailed' ? 90 : 65,
    })
  }

  return rows
}

/** 从恢复记录派生日历事件；已持久化成功的记录不再提示待办。 */
function eventFromOrderAttempt(attempt: OrderAttempt): CalendarEvent | undefined {
  if (attempt.status === 'persisted' || attempt.status === 'spot_persisted') {
    return undefined
  }

  return {
    id: `recovery-${attempt.id}`,
    type: 'recovery',
    date: localDateKey(attempt.updatedAt),
    timeLabel: localTimeLabel(attempt.updatedAt),
    title: attempt.status.includes('failed') ? t('ui.pendingOrderRecovery') : t('ui.orderAttemptRecord'),
    detail: `${attempt.optionSymbol} · ${attemptStatusText(attempt.status)} · ${messageText(attempt.message)}`,
    tone: attempt.status.includes('failed') ? 'red' : 'gray',
    targetView: 'orders',
    priority: attempt.status.includes('failed') ? 92 : 52,
  }
}

/** 从资金恢复记录派生日历事件；已确认和明确失败的终态不再提示待办。 */
function eventFromFundingAttempt(attempt: FundingAttemptView): CalendarEvent | undefined {
  if (attempt.status === 'confirmed' || attempt.status === 'failed') {
    return undefined
  }

  return {
    id: `funding-recovery-${attempt.id}`,
    type: 'recovery',
    date: localDateKey(attempt.updatedAt),
    timeLabel: localTimeLabel(attempt.updatedAt),
    title: attempt.status === 'blocked' ? t('ui.pendingFundingRecovery') : t('ui.fundingRecoveryRecord'),
    detail: `${messageText(attempt.kindLabel)} ${attempt.amount} ${attempt.asset} · ${fundingAttemptStatusText(attempt.status)} · ${messageText(attempt.message)}`,
    tone: attempt.status === 'blocked' ? 'red' : 'gray',
    targetView: 'orders',
    priority: attempt.status === 'blocked' ? 94 : 54,
  }
}

function hasRebalanceEvent(order: StrategyOrder) {
  // 只要出现过现货补齐订单引用或成交数量，就展示补齐事件，方便追踪 ITM 后续动作。
  return order.status === 'rebalanceFailed'
    || Boolean(order.spotOrderId)
    || Boolean(order.spotOrderStatus)
    || Boolean(order.spotClientOrderId)
    || decimalStringHasValue(order.spotFilledQuantity)
}

/** 同日事件按优先级高的先展示。 */
function compareEvents(left: CalendarEvent, right: CalendarEvent) {
  const dateCompare = left.date.localeCompare(right.date)
  if (dateCompare !== 0)
    return dateCompare
  if (left.priority !== right.priority)
    return right.priority - left.priority
  return (left.timeLabel ?? '').localeCompare(right.timeLabel ?? '')
}

/** 判断日期键是否位于可见范围内。 */
function isDateInRange(dateKey: string, start: Date, end: Date) {
  const date = parseDateKey(dateKey)
  return date >= start && date <= end
}
