// 从净仓和交割任务生成日历事件，订单历史不计为当前持仓。

import type { MaybeRefOrGetter } from 'vue'
import type { AppViewId } from '@/stores/app'
import type { DeliveryTask, FundingAttemptView, OrderAttempt, TrackedPosition } from '@/types/domain'
import { computed, toValue } from 'vue'
import { messageText, t } from '@/i18n'
import { useAppStore } from '@/stores/app'
import {
  localDateKey,
  localTimeLabel,
  parseDateKey,
  startOfDay,
} from '@/utils/date'
import { attemptStatusText, deliveryStatusText, fundingAttemptStatusText, positionStatusText } from '@/utils/labels'

/** 日历事件类型只用于筛选和跳转，不对应独立持久化任务。 */
export type CalendarEventType
  = | 'position'
    | 'expiry'
    | 'delivery'
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
  { id: 'position', get label() { return t('ui.openShortPositions') }, tone: 'gold' },
  { id: 'expiry', get label() { return t('ui.expiry') }, tone: 'purple' },
  { id: 'delivery', get label() { return t('ui.deliveryEvent') }, tone: 'green' },
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

    // 净仓按完整合约代码唯一，一份义务只有一个到期事件。
    for (const position of store.positions) {
      rows.push(...eventsFromPosition(position))
    }
    for (const task of store.deliveries) {
      rows.push(eventFromDelivery(task))
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

/** 登记和到期以净仓为准；已提前平仓的合约不再展示到期义务。 */
function eventsFromPosition(position: TrackedPosition): CalendarEvent[] {
  const direction = position.direction === 'sellPut' ? t('ui.buyBtcLow') : t('ui.sellBtcHigh2')
  const rows: CalendarEvent[] = [
    {
      id: `position-${position.optionSymbol}`,
      type: 'position',
      date: localDateKey(position.trackedAt),
      timeLabel: localTimeLabel(position.trackedAt),
      title: t('ui.positionTracked'),
      detail: t('ui.positionEventDetail', { p0: position.optionSymbol, p1: position.quantity, p2: positionStatusText(position.status) }),
      tone: 'gold',
      targetView: 'portfolio',
      priority: 30,
    },
  ]

  if (position.status !== 'closed') {
    rows.push({
      id: `expiry-${position.optionSymbol}`,
      type: 'expiry',
      date: localDateKey(position.expiryAt),
      timeLabel: localTimeLabel(position.expiryAt),
      title: t('ui.expiry2', { p0: direction }),
      detail: t('ui.strikeStatus', { p0: position.optionSymbol, p1: position.strike, p2: positionStatusText(position.status) }),
      tone: 'purple',
      targetView: position.status === 'expired' ? 'orders' : 'portfolio',
      priority: 70,
    })
  }
  return rows
}

function eventFromDelivery(task: DeliveryTask): CalendarEvent {
  const timestamp = task.completedAt ?? task.updatedAt
  return {
    id: `delivery-${task.id}`,
    type: 'delivery',
    date: localDateKey(timestamp),
    timeLabel: localTimeLabel(timestamp),
    title: deliveryStatusText(task.status),
    detail: `${task.optionSymbol} · ${messageText(task.message)}`,
    tone: task.status === 'failed' ? 'red' : task.status === 'completed' ? 'green' : 'gold',
    targetView: 'orders',
    priority: task.status === 'failed' ? 90 : 65,
  }
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
