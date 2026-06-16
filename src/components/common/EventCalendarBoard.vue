<!--
事件日历看板。

组件只负责月份、类型筛选和详情列表交互；事件本身由 `useCalendarEvents`
从当前快照派生，不从日志文件或其他旁路数据源推断。
-->
<script setup lang="ts">
import type { CalendarEvent, CalendarEventTone, CalendarEventType } from '@/composables/useCalendarEvents'

import {
  ChevronLeft,
  ChevronRight,
  Clock3,
} from '@lucide/vue'
import { NButton, NIcon } from 'naive-ui'
import { computed, shallowRef, watch } from 'vue'
import {
  calendarEventTypes,
  useCalendarEvents,
} from '@/composables/useCalendarEvents'
import { useLocalToday } from '@/composables/useLocalToday'
import { formatLocale, t } from '@/i18n'
import { useAppStore } from '@/stores/app'
import {
  addDays,
  addMonths,
  formatDateKey,
  isSameMonth,
  parseDateKey,
  startOfMonth,
  startOfWeek,
} from '@/utils/date'

interface CalendarDay {
  date: string
  label: number
  inCurrentMonth: boolean
  isToday: boolean
  events: CalendarEvent[]
}

const store = useAppStore()
const weekDays = computed(() => [t('ui.sun'), t('ui.mon'), t('ui.tue'), t('ui.wed'), t('ui.thu'), t('ui.fri'), t('ui.sat')])
const { today, todayKey } = useLocalToday()

// 月历状态只控制可见窗口和选中日期，不会创建或修改任何真实待办。
const currentMonth = shallowRef(startOfMonth(today.value))
const selectedDate = shallowRef(todayKey.value)
const activeType = shallowRef<CalendarEventType | 'all'>('all')

watch(todayKey, (nextDay, previousDay) => {
  // 正在查看今天时跟随换日；用户选中的其他日期保持原位。
  if (selectedDate.value === previousDay) {
    currentMonth.value = startOfMonth(today.value)
    selectedDate.value = nextDay
  }
})

const visibleStart = computed(() => startOfWeek(currentMonth.value))
const visibleEnd = computed(() => addDays(visibleStart.value, 41))

// 只请求当前 6 周网格范围内的派生事件，避免模板里重复扫描全量订单/恢复记录。
const { events } = useCalendarEvents({
  startDate: visibleStart,
  endDate: visibleEnd,
})

const filteredEvents = computed(() => {
  if (activeType.value === 'all') {
    return events.value
  }
  return events.value.filter(event => event.type === activeType.value)
})

const eventsByDate = computed(() => {
  // 先按当前筛选类型分组，右侧详情和月历角标才能保持同一口径。
  const map = new Map<string, CalendarEvent[]>()
  for (const event of filteredEvents.value) {
    const rows = map.get(event.date) ?? []
    rows.push(event)
    map.set(event.date, rows)
  }
  return map
})

const calendarDays = computed<CalendarDay[]>(() => {
  // 固定 6 周网格，月份切换时尺寸稳定，避免日历区域跳动。
  return Array.from({ length: 42 }, (_, index) => {
    const day = addDays(visibleStart.value, index)
    const date = formatDateKey(day)
    return {
      date,
      label: day.getDate(),
      inCurrentMonth: day.getMonth() === currentMonth.value.getMonth(),
      isToday: date === todayKey.value,
      events: eventsByDate.value.get(date) ?? [],
    }
  })
})

const selectedDayEvents = computed(() =>
  eventsByDate.value.get(selectedDate.value) ?? [],
)

const monthTitle = computed(() => new Intl.DateTimeFormat(formatLocale(), { year: 'numeric', month: 'long', calendar: 'gregory' }).format(currentMonth.value))

const selectedDateTitle = computed(() => {
  const date = parseDateKey(selectedDate.value)
  return date.toLocaleDateString(formatLocale(), {
    month: 'long',
    day: 'numeric',
    weekday: 'long',
  })
})

function selectDate(date: string) {
  selectedDate.value = date
}

function previousMonth() {
  setCurrentMonth(addMonths(currentMonth.value, -1))
}

function nextMonth() {
  setCurrentMonth(addMonths(currentMonth.value, 1))
}

function selectToday() {
  currentMonth.value = startOfMonth(today.value)
  selectedDate.value = todayKey.value
}

function setCurrentMonth(date: Date) {
  // 切到本月时回到今天；切到其他月份时选中月初，避免保留一个不可见的旧日期。
  currentMonth.value = startOfMonth(date)
  selectedDate.value = isSameMonth(today.value, currentMonth.value)
    ? todayKey.value
    : formatDateKey(currentMonth.value)
}

function typeLabel(type: CalendarEventType) {
  return calendarEventTypes.find(item => item.id === type)?.label ?? t('ui.events')
}

function toneClass(tone: CalendarEventTone) {
  return `tone-${tone}`
}
</script>

<template>
  <section class="event-calendar-board">
    <div class="event-calendar-toolbar">
      <div class="event-calendar-heading">
        <div>
          <strong>{{ monthTitle }}</strong>
        </div>
      </div>

      <div class="event-calendar-controls">
        <NButton circle secondary size="small" :title="t('ui.previousMonth')" :aria-label="t('ui.previousMonth')" @click="previousMonth">
          <template #icon><NIcon :component="ChevronLeft" /></template>
        </NButton>
        <NButton size="small" secondary @click="selectToday"> {{ t('ui.today') }} </NButton>
        <NButton circle secondary size="small" :title="t('ui.nextMonth')" :aria-label="t('ui.nextMonth')" @click="nextMonth">
          <template #icon><NIcon :component="ChevronRight" /></template>
        </NButton>
      </div>
    </div>

    <div class="event-calendar-layout">
      <div class="event-calendar-month">
        <div class="event-calendar-weekdays">
          <span v-for="day in weekDays" :key="day">{{ day }}</span>
        </div>

        <div class="event-calendar-grid">
          <button
            v-for="day in calendarDays"
            :key="day.date"
            class="event-calendar-day"
            :class="{
              'outside': !day.inCurrentMonth,
              'today': day.isToday,
              'selected': selectedDate === day.date,
              'has-events': day.events.length > 0,
            }"
            type="button"
            @click="selectDate(day.date)"
          >
            <span class="event-day-number">{{ day.label }}</span>
            <span v-if="day.events.length > 0" class="event-day-count">{{ day.events.length }}</span>
            <span v-if="day.events.length > 0" class="event-day-dots">
              <i
                v-for="event in day.events.slice(0, 3)"
                :key="event.id"
                :class="toneClass(event.tone)"
              />
            </span>
          </button>
        </div>

        <div class="event-calendar-filters" :aria-label="t('ui.filterEvents')">
          <button
            v-for="type in calendarEventTypes"
            :key="type.id"
            class="event-calendar-filter"
            :class="{ active: activeType === type.id }"
            type="button"
            @click="activeType = type.id"
          >
            <span class="event-filter-dot" :class="toneClass(type.tone)" />
            <span>{{ type.label }}</span>
          </button>
        </div>
      </div>

      <aside class="event-calendar-detail">
        <header class="event-detail-header">
          <div>
            <span> {{ t('ui.eventDetails') }} </span>
            <strong>{{ selectedDateTitle }}</strong>
          </div>
        </header>

        <p v-if="selectedDayEvents.length === 0" class="empty-state"> {{ t('ui.noEventsMatchThisFilter') }} </p>

        <div v-else class="event-detail-list">
          <article
            v-for="event in selectedDayEvents"
            :key="event.id"
            class="event-detail-card"
            :class="toneClass(event.tone)"
          >
            <div class="event-detail-meta">
              <span>{{ typeLabel(event.type) }}</span>
              <em>
                <NIcon :component="Clock3" />
                {{ event.timeLabel ?? t('ui.allDay') }}
              </em>
            </div>
            <strong>{{ event.title }}</strong>
            <p>{{ event.detail }}</p>
            <NButton size="small" secondary @click="store.openView(event.targetView)"> {{ t('ui.view') }} </NButton>
          </article>
        </div>
      </aside>
    </div>
  </section>
</template>
