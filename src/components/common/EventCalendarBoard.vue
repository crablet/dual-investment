<script setup lang="ts">
// 事件日历看板。
// 组件只负责月份、类型筛选和详情列表交互；事件本身由 `useCalendarEvents` 从快照派生。
import {
  ChevronLeft,
  ChevronRight,
  Clock3,
} from '@lucide/vue'
import { NButton, NIcon } from 'naive-ui'
import { computed, shallowRef } from 'vue'
import {
  calendarEventTypes,
  useCalendarEvents,
  type CalendarEvent,
  type CalendarEventTone,
  type CalendarEventType,
} from '@/composables/useCalendarEvents'
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
const weekDays = ['周日', '周一', '周二', '周三', '周四', '周五', '周六']
const today = new Date()
const todayKey = formatDateKey(today)

const currentMonth = shallowRef(startOfMonth(today))
const selectedDate = shallowRef(todayKey)
const activeType = shallowRef<CalendarEventType | 'all'>('all')

const visibleStart = computed(() => startOfWeek(currentMonth.value))
const visibleEnd = computed(() => addDays(visibleStart.value, 41))
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
  return Array.from({ length: 42 }, (_unused, index) => {
    const day = addDays(visibleStart.value, index)
    const date = formatDateKey(day)
    return {
      date,
      label: day.getDate(),
      inCurrentMonth: day.getMonth() === currentMonth.value.getMonth(),
      isToday: date === todayKey,
      events: eventsByDate.value.get(date) ?? [],
    }
  })
})

const selectedDayEvents = computed(() =>
  eventsByDate.value.get(selectedDate.value) ?? [],
)

const monthTitle = computed(() =>
  `${currentMonth.value.getFullYear()}年${currentMonth.value.getMonth() + 1}月`,
)

const selectedDateTitle = computed(() => {
  const date = parseDateKey(selectedDate.value)
  return date.toLocaleDateString('zh-CN', {
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
  currentMonth.value = startOfMonth(today)
  selectedDate.value = todayKey
}

function setCurrentMonth(date: Date) {
  currentMonth.value = startOfMonth(date)
  selectedDate.value = isSameMonth(today, currentMonth.value)
    ? todayKey
    : formatDateKey(currentMonth.value)
}

function typeLabel(type: CalendarEventType) {
  return calendarEventTypes.find(item => item.id === type)?.label ?? '事件'
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
        <NButton circle secondary size="small" title="上一月" aria-label="上一月" @click="previousMonth">
          <template #icon><NIcon :component="ChevronLeft" /></template>
        </NButton>
        <NButton size="small" secondary @click="selectToday">
          今天
        </NButton>
        <NButton circle secondary size="small" title="下一月" aria-label="下一月" @click="nextMonth">
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
              outside: !day.inCurrentMonth,
              today: day.isToday,
              selected: selectedDate === day.date,
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

        <div class="event-calendar-filters" aria-label="事件筛选">
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
            <span>事件详情</span>
            <strong>{{ selectedDateTitle }}</strong>
          </div>
        </header>

        <p v-if="selectedDayEvents.length === 0" class="empty-state">
          当前筛选下暂无事件
        </p>

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
                {{ event.timeLabel ?? '全天' }}
              </em>
            </div>
            <strong>{{ event.title }}</strong>
            <p>{{ event.detail }}</p>
            <NButton size="small" secondary @click="store.openView(event.targetView)">
              查看
            </NButton>
          </article>
        </div>
      </aside>
    </div>
  </section>
</template>
