<script setup lang="ts">
import type { StrategyCandidate, StrategyDirection } from '@/types/domain'
// 双币赢工作台。
// 统一承载候选浏览、手动下单和自动复投规则；没有独立自动复投页面或 mock 数据路径。
import {
  ArrowDownCircle,
  ArrowUpCircle,
  ArrowUpDown,
  ChevronDown,
  ChevronUp,
  RefreshCw,
  Save,
  ShieldCheck,
  Zap,
} from '@lucide/vue'
import {
  NButton,
  NCheckbox,
  NIcon,
  NInputNumber,
  NSlider,
  NSwitch,
} from 'naive-ui'
import { computed, reactive, shallowRef, watch, watchEffect } from 'vue'
import OrderProgressPanel from '@/components/common/OrderProgressPanel.vue'
import { useFormat } from '@/composables/useFormat'
import { useAppStore } from '@/stores/app'
import { localDateTimeLabel } from '@/utils/date'

interface RuleForm {
  enabled: boolean
  schedule: string
  quantity: number
  minAnnualized: number
  maxDeltaAbs: number
  maxIvRank: number
  minDaysToExpiry: number
  maxDaysToExpiry: number
  includeBtc: boolean
  excludeSameDirection: boolean
}

interface CandidateFilterForm {
  minAnnualized: number | null
  maxExerciseProbability: number | null
  maxDistanceAbs: number | null
}

type CandidateSortKey = 'strike' | 'annualized' | 'expiry' | 'limitPrice' | 'exerciseProbability'
type CandidateSortDirection = 'asc' | 'desc'

const store = useAppStore()
const { money, percent } = useFormat()

const dayMs = 86_400_000
const expiryFilters = [
  { id: 'all', label: '所有到期日' },
  { id: 'lt3', label: '< 3 天' },
  { id: 'lt7', label: '< 7 天' },
  { id: 'd7_30', label: '7-30 天' },
  { id: 'd30_60', label: '30-60 天' },
  { id: 'gt60', label: '> 60 天' },
] as const

const candidateSortColumns: Array<{ key: CandidateSortKey, label: string }> = [
  { key: 'strike', label: '目标价' },
  { key: 'annualized', label: '年化' },
  { key: 'expiry', label: '到期' },
  { key: 'limitPrice', label: '标记价格' },
  { key: 'exerciseProbability', label: '被行权概率' },
]

const candidateSortDefaultDirection: Record<CandidateSortKey, CandidateSortDirection> = {
  strike: 'asc',
  annualized: 'desc',
  expiry: 'asc',
  limitPrice: 'desc',
  exerciseProbability: 'asc',
}

type ExpiryFilter = typeof expiryFilters[number]['id']

const selectedCandidateId = shallowRef<string>()
const expiryFilter = shallowRef<ExpiryFilter>('all')
const candidateSortKey = shallowRef<CandidateSortKey>('annualized')
const candidateSortDirection = shallowRef<CandidateSortDirection>('desc')
const advancedAutoOpen = shallowRef(false)

const form = reactive({
  direction: 'sellPut' as StrategyDirection,
  quantity: 0.01,
})

const ruleForm = reactive<RuleForm>({
  enabled: false,
  schedule: '每天 11:30',
  quantity: 0.01,
  minAnnualized: 15,
  maxDeltaAbs: 0.30,
  maxIvRank: 60,
  minDaysToExpiry: 7,
  maxDaysToExpiry: 45,
  includeBtc: true,
  excludeSameDirection: true,
})

const candidateFilter = reactive<CandidateFilterForm>({
  minAnnualized: null,
  maxExerciseProbability: null,
  maxDistanceAbs: null,
})

watchEffect(() => {
  if (!store.autoRule) {
    return
  }
  Object.assign(ruleForm, {
    ...store.autoRule,
    quantity: Number(store.autoRule.quantity),
    minAnnualized: Number(store.autoRule.minAnnualized),
    maxDeltaAbs: Number(store.autoRule.maxDeltaAbs),
    maxIvRank: Number(store.autoRule.maxIvRank),
  })
})

const selectedCandidate = computed(() =>
  store.candidates.find(candidate => candidate.id === selectedCandidateId.value),
)

const filteredCandidates = computed(() => {
  // 前端筛选只影响展示。候选生成和方向/现货价硬约束已经在 Rust 策略层完成。
  return store.candidates
    .filter(candidate => candidate.direction === form.direction)
    .filter(matchesExpiryFilter)
    .filter(matchesCandidateFilters)
})

const sortedCandidates = computed(() => {
  // 排序保持稳定：字段相同时使用扫描顺序，避免列表在同一批数据中跳动。
  return filteredCandidates.value
    .map((candidate, index) => ({ candidate, index }))
    .sort((left, right) => {
      const modifier = candidateSortDirection.value === 'asc' ? 1 : -1
      const delta = compareCandidateSortValue(
        left.candidate,
        right.candidate,
        candidateSortKey.value,
      )
      return delta === 0 ? left.index - right.index : delta * modifier
    })
    .map(item => item.candidate)
})

const activeCandidateFilterCount = computed(() =>
  [
    candidateFilter.minAnnualized,
    candidateFilter.maxExerciseProbability,
    candidateFilter.maxDistanceAbs,
  ].filter(value => value !== null).length,
)

const referenceSpotPrice = computed(() =>
  store.candidates.find(candidate => Number(candidate.spotPrice) > 0)?.spotPrice,
)

const directionStats = computed(() => ({
  sellPut: store.candidates.filter(candidate => candidate.direction === 'sellPut').length,
  sellCall: store.candidates.filter(candidate => candidate.direction === 'sellCall').length,
}))

const panelTitle = computed(() =>
  form.direction === 'sellPut' ? '以 USDT 低买 BTC' : '以 BTC 高卖 BTC',
)

const activeDirectionLabel = computed(() =>
  form.direction === 'sellPut' ? '低买' : '高卖',
)

const otherDirectionLabel = computed(() =>
  form.direction === 'sellPut' ? '高卖' : '低买',
)

const activeOptionType = computed(() =>
  form.direction === 'sellPut' ? 'Put' : 'Call',
)

const currentDirectionCandidateCount = computed(() =>
  form.direction === 'sellPut' ? directionStats.value.sellPut : directionStats.value.sellCall,
)

const otherDirectionCandidateCount = computed(() =>
  form.direction === 'sellPut' ? directionStats.value.sellCall : directionStats.value.sellPut,
)

const emptyCandidateTitle = computed(() => {
  if (store.candidates.length === 0)
    return '暂无候选'
  if (activeCandidateFilterCount.value > 0)
    return '当前筛选没有候选'
  if (currentDirectionCandidateCount.value > 0)
    return '当前到期日没有候选'
  return `暂无${activeDirectionLabel.value}（${activeOptionType.value}）候选`
})

const emptyCandidateHint = computed(() => {
  if (store.candidates.length === 0)
    return '当前还没有候选，请先重新扫描。'
  if (activeCandidateFilterCount.value > 0) {
    return '可以放宽年化、被行权概率或距现货筛选，也可以一键重置筛选条件。'
  }
  if (currentDirectionCandidateCount.value === 0 && otherDirectionCandidateCount.value > 0) {
    return `本次扫描只有${otherDirectionLabel.value}候选，当前没有${activeDirectionLabel.value}候选。可以放宽自动复投规则，或切换到${otherDirectionLabel.value}查看。`
  }
  return '当前到期日筛选没有命中；候选浏览范围为 1-90 天，可以切换到所有到期日，或重新扫描候选。'
})

const projectedPremium = computed(() => {
  // 右侧卡片的预计权利金只是 UI 估算；真实权利金以 Binance 成交回报为准。
  if (!selectedCandidate.value)
    return 0
  return Number(selectedCandidate.value.limitPrice) * form.quantity
})

const projectedBreakeven = computed(() => {
  if (!selectedCandidate.value)
    return 0
  const strike = Number(selectedCandidate.value.strike)
  const price = Number(selectedCandidate.value.limitPrice)
  return selectedCandidate.value.direction === 'sellPut'
    ? strike - price
    : strike + price
})

const canSubmit = computed(() =>
  Boolean(store.settings?.connected)
  && Boolean(selectedCandidate.value)
  && form.quantity >= 0.01,
)

watch(
  sortedCandidates,
  (candidates) => {
    if (selectedCandidateId.value && candidates.some(candidate => candidate.id === selectedCandidateId.value)) {
      return
    }
    if (candidates[0]) {
      selectCandidate(candidates[0])
      return
    }
    selectedCandidateId.value = undefined
  },
  { immediate: true },
)

function daysUntil(expiry: string) {
  const target = expiry.includes('T') ? new Date(expiry) : new Date(`${expiry}T00:00:00`)
  return Math.max(0, Math.ceil((target.getTime() - Date.now()) / dayMs))
}

function matchesExpiryFilter(candidate: StrategyCandidate) {
  const days = daysUntil(candidate.expiryAt)
  if (expiryFilter.value === 'lt3')
    return days < 3
  if (expiryFilter.value === 'lt7')
    return days < 7
  if (expiryFilter.value === 'd7_30')
    return days >= 7 && days <= 30
  if (expiryFilter.value === 'd30_60')
    return days > 30 && days <= 60
  if (expiryFilter.value === 'gt60')
    return days > 60
  return true
}

function matchesCandidateFilters(candidate: StrategyCandidate) {
  const annualized = Number(candidate.annualized)
  const exerciseProbability = Number(candidate.exerciseProbability)
  const distanceAbs = Math.abs(Number(candidate.strikeDistancePercent))

  if (candidateFilter.minAnnualized !== null && annualized < candidateFilter.minAnnualized) {
    return false
  }
  if (
    candidateFilter.maxExerciseProbability !== null
    && exerciseProbability > candidateFilter.maxExerciseProbability
  ) {
    return false
  }
  if (candidateFilter.maxDistanceAbs !== null && distanceAbs > candidateFilter.maxDistanceAbs) {
    return false
  }
  return true
}

function candidateSortNumber(candidate: StrategyCandidate, key: CandidateSortKey) {
  if (key === 'expiry') {
    return new Date(candidate.expiryAt).getTime()
  }
  return Number(candidate[key])
}

function compareCandidateSortValue(
  left: StrategyCandidate,
  right: StrategyCandidate,
  key: CandidateSortKey,
) {
  const leftValue = candidateSortNumber(left, key)
  const rightValue = candidateSortNumber(right, key)
  if (!Number.isFinite(leftValue) && !Number.isFinite(rightValue))
    return 0
  if (!Number.isFinite(leftValue))
    return 1
  if (!Number.isFinite(rightValue))
    return -1
  return leftValue - rightValue
}

function toggleCandidateSort(key: CandidateSortKey) {
  if (candidateSortKey.value === key) {
    candidateSortDirection.value = candidateSortDirection.value === 'asc' ? 'desc' : 'asc'
    return
  }
  candidateSortKey.value = key
  candidateSortDirection.value = candidateSortDefaultDirection[key]
}

function selectDirection(direction: StrategyDirection) {
  if (form.direction === direction) {
    return
  }
  form.direction = direction
  selectedCandidateId.value = undefined
}

function selectCandidate(candidate: StrategyCandidate) {
  selectedCandidateId.value = candidate.id
  form.direction = candidate.direction
  form.quantity = Number(candidate.quantity)
}

function resetCandidateFilters() {
  candidateFilter.minAnnualized = null
  candidateFilter.maxExerciseProbability = null
  candidateFilter.maxDistanceAbs = null
}

function signedPercent(value: string | number) {
  const numeric = Number(value)
  if (!Number.isFinite(numeric))
    return '--'
  const sign = numeric > 0 ? '+' : ''
  return `${sign}${percent(numeric)}`
}

function saveAutoRule() {
  // 表单用 number 便于 slider/input 交互，提交前转回 Decimal 字符串契约。
  void store.saveRule({
    ...ruleForm,
    quantity: ruleForm.quantity.toFixed(2),
    minAnnualized: String(ruleForm.minAnnualized),
    maxDeltaAbs: ruleForm.maxDeltaAbs.toFixed(2),
    maxIvRank: String(ruleForm.maxIvRank),
  })
}

function toggleAuto(value: boolean) {
  // 自动复投开启后符合条件的自动单不会逐笔确认，因此开关本身必须二次确认。
  if (value && !window.confirm('确认开启自动定投？后续符合条件的自动单不会逐笔确认。')) {
    return
  }
  void store.toggleAuto({
    enabled: value,
    confirmLive: true,
  })
}

function runAutoNow() {
  // 立即执行可能直接下真实订单，必须显式确认。
  if (!window.confirm('确认立即执行一次自动扫描？若存在推荐候选，将直接发送 Binance 订单。')) {
    return
  }
  void store.runAutoOnce()
}

function submit() {
  const candidate = selectedCandidate.value
  if (!candidate) {
    return
  }
  // 手动下单也走真实 Binance 订单，确认后才传 `confirmLive: true`。
  if (!window.confirm('确认发送 Binance 期权订单？')) {
    return
  }
  void store.placeStrategy({
    candidateId: candidate.id,
    symbol: candidate.symbol,
    optionSymbol: candidate.optionSymbol,
    direction: candidate.direction,
    expiry: candidate.expiry,
    expiryAt: candidate.expiryAt,
    strike: candidate.strike,
    quantity: form.quantity.toFixed(2),
    limitPrice: candidate.limitPrice,
    confirmLive: true,
  })
}
</script>

<template>
  <div class="dual-workbench">
    <div class="deal-main">
      <section class="direction-switch">
        <button :class="{ active: form.direction === 'sellPut' }" type="button" @click="selectDirection('sellPut')">
          <NIcon :component="ArrowDownCircle" />
          <span>低买</span>
          <strong>卖 Put 抄底</strong>
        </button>
        <button :class="{ active: form.direction === 'sellCall' }" type="button" @click="selectDirection('sellCall')">
          <NIcon :component="ArrowUpCircle" />
          <span>高卖</span>
          <strong>卖 Call 止盈</strong>
        </button>
      </section>

      <section class="candidate-board">
        <div class="candidate-toolbar">
          <div class="expiry-tabs" aria-label="到期日筛选">
            <button
              v-for="filter in expiryFilters"
              :key="filter.id"
              :class="{ active: expiryFilter === filter.id }"
              type="button"
              @click="expiryFilter = filter.id"
            >
              {{ filter.label }}
            </button>
          </div>

          <div class="candidate-toolbar-actions">
            <span v-if="referenceSpotPrice" class="spot-price">BTC 现货 {{ money(referenceSpotPrice) }}</span>
            <NButton
              circle
              secondary
              size="small"
              title="刷新候选"
              aria-label="刷新候选"
              :loading="store.isLoading('scan')"
              @click="store.scan()"
            >
              <template #icon><NIcon :component="RefreshCw" /></template>
            </NButton>
          </div>
        </div>

        <div class="candidate-filter-bar" aria-label="候选筛选">
          <label>
            年化 ≥
            <NInputNumber
              v-model:value="candidateFilter.minAnnualized"
              clearable
              placeholder="不限"
              :min="0"
              :step="1"
            />
          </label>
          <label>
            被行权概率 ≤
            <NInputNumber
              v-model:value="candidateFilter.maxExerciseProbability"
              clearable
              placeholder="不限"
              :min="0"
              :max="100"
              :step="1"
            />
          </label>
          <label>
            距现货 ≤
            <NInputNumber
              v-model:value="candidateFilter.maxDistanceAbs"
              clearable
              placeholder="不限"
              :min="0"
              :step="0.5"
            />
          </label>
          <NButton secondary @click="resetCandidateFilters">重置</NButton>
        </div>

        <div class="opportunity-table">
          <div class="opportunity-row opportunity-head">
            <button
              v-for="column in candidateSortColumns"
              :key="column.key"
              class="opportunity-sort"
              :class="{ active: candidateSortKey === column.key }"
              type="button"
              :aria-pressed="candidateSortKey === column.key"
              @click="toggleCandidateSort(column.key)"
            >
              <span>{{ column.label }}</span>
              <NIcon
                :component="candidateSortKey === column.key
                  ? (candidateSortDirection === 'asc' ? ChevronUp : ChevronDown)
                  : ArrowUpDown"
              />
            </button>
          </div>
          <button
            v-for="candidate in sortedCandidates"
            :key="candidate.id"
            class="opportunity-row"
            :class="{ selected: selectedCandidate?.id === candidate.id }"
            type="button"
            @click="selectCandidate(candidate)"
          >
            <span>
              <strong>{{ money(candidate.strike) }}</strong>
              <em>
                距现货 {{ signedPercent(candidate.strikeDistancePercent) }}
              </em>
            </span>
            <span class="positive">{{ percent(candidate.annualized) }}</span>
            <span>
              <strong>{{ localDateTimeLabel(candidate.expiryAt) }}</strong>
              <em>{{ daysUntil(candidate.expiryAt) }} 天</em>
            </span>
            <span>{{ money(candidate.limitPrice) }}</span>
            <span>{{ percent(candidate.exerciseProbability) }}</span>
          </button>
        </div>

        <div v-if="filteredCandidates.length === 0" class="empty-state opportunity-empty">
          <strong>{{ emptyCandidateTitle }}</strong>
          <span>{{ emptyCandidateHint }}</span>
          <NButton size="small" secondary :loading="store.isLoading('scan')" @click="store.scan()">重新扫描</NButton>
        </div>
      </section>
    </div>

    <aside class="order-panel">
      <section class="order-card">
        <header>
          <div>
            <span class="order-eyebrow">下单</span>
            <h3>{{ panelTitle }}</h3>
          </div>
          <NIcon :component="Zap" />
        </header>

        <div class="deal-summary">
          <article>
            <span>年化</span>
            <strong :class="{ positive: selectedCandidate }">
              {{ selectedCandidate ? percent(selectedCandidate.annualized) : '--' }}
            </strong>
            <em v-if="selectedCandidate && Number(selectedCandidate.spotPrice) > 0">
              距现货 {{ signedPercent(selectedCandidate.strikeDistancePercent) }}
            </em>
          </article>
          <article>
            <span>到期日</span>
            <strong>{{ selectedCandidate ? localDateTimeLabel(selectedCandidate.expiryAt) : '--' }}</strong>
            <em v-if="selectedCandidate">{{ daysUntil(selectedCandidate.expiryAt) }} 天</em>
          </article>
          <article>
            <span>预计权利金</span>
            <strong>{{ money(String(projectedPremium)) }}</strong>
          </article>
          <article>
            <span>保本价</span>
            <strong>{{ money(String(projectedBreakeven)) }}</strong>
          </article>
        </div>

        <div class="order-form">
          <label>
            下单数量（张）
            <NInputNumber v-model:value="form.quantity" :min="0.01" :step="0.01" />
          </label>
        </div>

        <NButton type="primary" size="large" block :disabled="!canSubmit" :loading="store.isLoading('order')" @click="submit">
          <template #icon><NIcon :component="Zap" /></template>
          发送订单
        </NButton>

        <OrderProgressPanel
          :progress="store.orderProgress"
          @close="store.clearOrderProgress"
          @view-orders="store.openView('orders')"
        />
      </section>

      <section class="auto-card">
        <header>
          <div>
            <span class="order-eyebrow">自动复投</span>
            <h3>{{ store.settings?.autoEnabled ? '已启用' : '未启用' }}</h3>
          </div>
          <NSwitch :value="store.settings?.autoEnabled ?? false" @update:value="toggleAuto" />
        </header>

        <div class="auto-card-body">
          <label>
            扫描计划
            <input v-model="ruleForm.schedule" class="native-input">
          </label>
          <label>
            每次下单数量
            <NInputNumber v-model:value="ruleForm.quantity" :min="0.01" :step="0.01" />
          </label>

          <button class="advanced-toggle" type="button" @click="advancedAutoOpen = !advancedAutoOpen">
            <span>筛选条件</span>
            <strong>{{ percent(ruleForm.minAnnualized) }} 起</strong>
          </button>

          <div v-if="advancedAutoOpen" class="auto-rule-grid">
            <label>
              预计年化 ≥ {{ percent(ruleForm.minAnnualized) }}
              <NSlider v-model:value="ruleForm.minAnnualized" :min="5" :max="60" :step="1" />
            </label>
            <label>
              Delta 绝对值 ≤ {{ ruleForm.maxDeltaAbs }}
              <NSlider v-model:value="ruleForm.maxDeltaAbs" :min="0.05" :max="0.8" :step="0.01" />
            </label>
            <label>
              Mark IV 代理 ≤ {{ percent(ruleForm.maxIvRank, 0) }}
              <NSlider v-model:value="ruleForm.maxIvRank" :min="10" :max="100" :step="1" />
            </label>
            <div class="range-row compact-range">
              <span>到期</span>
              <NInputNumber v-model:value="ruleForm.minDaysToExpiry" :min="1" />
              <span>至</span>
              <NInputNumber v-model:value="ruleForm.maxDaysToExpiry" :min="ruleForm.minDaysToExpiry" />
              <span>天</span>
            </div>
            <div class="checkbox-row">
              <NCheckbox v-model:checked="ruleForm.includeBtc">BTC</NCheckbox>
              <NCheckbox v-model:checked="ruleForm.excludeSameDirection">排除同方向持仓</NCheckbox>
            </div>
          </div>
        </div>

        <div class="auto-actions">
          <NButton secondary :loading="store.isLoading('rule')" @click="saveAutoRule">
            <template #icon><NIcon :component="Save" /></template>
            保存规则
          </NButton>
          <NButton secondary :disabled="!store.settings?.autoEnabled" :loading="store.isLoading('auto')" @click="runAutoNow">
            <template #icon><NIcon :component="ShieldCheck" /></template>
            执行一次
          </NButton>
        </div>
      </section>
    </aside>
  </div>
</template>
