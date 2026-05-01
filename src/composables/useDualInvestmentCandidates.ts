// 双币赢候选列表的前端交互状态。
//
// Rust 策略层已经完成实盘合约过滤、方向硬约束和推荐等级计算；这个 composable 只负责
// 页面上的方向切换、到期/年化筛选、稳定排序、默认选中和下单卡片展示估算。

import type { StrategyCandidate, StrategyDirection } from '@/types/domain'
import { computed, reactive, shallowRef, watch } from 'vue'
import { useAppStore } from '@/stores/app'
import { decimalStringToDisplayNumber } from '@/utils/decimal'

interface CandidateFilterForm {
  minAnnualized: number | null
  maxExerciseProbability: number | null
  maxDistanceAbs: number | null
}

export type CandidateSortKey = 'strike' | 'annualized' | 'expiry' | 'limitPrice' | 'exerciseProbability'
export type CandidateSortDirection = 'asc' | 'desc'

// 到期筛选按自然天展示；实际期权到期和下单请求仍使用 Rust 返回的精确 expiryAt。
const dayMs = 86_400_000

export const expiryFilters = [
  { id: 'all', label: '所有到期日' },
  { id: 'lt3', label: '< 3 天' },
  { id: 'lt7', label: '< 7 天' },
  { id: 'd7_30', label: '7-30 天' },
  { id: 'd30_60', label: '30-60 天' },
  { id: 'gt60', label: '> 60 天' },
] as const

export const candidateSortColumns: Array<{ key: CandidateSortKey, label: string }> = [
  { key: 'strike', label: '目标价' },
  { key: 'annualized', label: '年化' },
  { key: 'expiry', label: '到期' },
  { key: 'limitPrice', label: '当前价格' },
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

/** 组合候选浏览、筛选、排序和右侧下单卡片所需的状态。 */
export function useDualInvestmentCandidates() {
  const store = useAppStore()
  const selectedCandidateId = shallowRef<string>()
  const expiryFilter = shallowRef<ExpiryFilter>('all')
  const candidateSortKey = shallowRef<CandidateSortKey>('annualized')
  const candidateSortDirection = shallowRef<CandidateSortDirection>('desc')

  const form = reactive({
    direction: 'sellPut' as StrategyDirection,
    quantity: 0.01,
  })

  const candidateFilter = reactive<CandidateFilterForm>({
    minAnnualized: null,
    maxExerciseProbability: null,
    maxDistanceAbs: null,
  })

  const selectedCandidate = computed(() =>
    store.candidates.find(candidate => candidate.id === selectedCandidateId.value),
  )

  const filteredCandidates = computed(() => {
    // 前端筛选只影响展示。候选生成和方向/现货价硬约束已经在 Rust 策略层完成。
    return store.candidates
      .filter(candidate => candidate.direction === form.direction)
      .filter(candidate => matchesExpiryFilter(candidate, expiryFilter.value))
      .filter(candidate => matchesCandidateFilters(candidate, candidateFilter))
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
    store.candidates.find(candidate => decimalStringToDisplayNumber(candidate.spotPrice) > 0)?.spotPrice,
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
    return decimalStringToDisplayNumber(selectedCandidate.value.limitPrice) * form.quantity
  })

  const projectedBreakeven = computed(() => {
    // 保本价预览帮助用户理解下单意图，不会写入订单事实；成交后以后端快照为准。
    if (!selectedCandidate.value)
      return 0
    const strike = decimalStringToDisplayNumber(selectedCandidate.value.strike)
    const price = decimalStringToDisplayNumber(selectedCandidate.value.limitPrice)
    return selectedCandidate.value.direction === 'sellPut'
      ? strike - price
      : strike + price
  })

  const canSubmit = computed(() => {
    // 前端只做基本可用性判断，真实合约精度、保证金和风控仍在 Rust 下单链路校验。
    return Boolean(store.settings?.connected)
      && Boolean(selectedCandidate.value)
      && form.quantity >= 0.01
  })

  watch(
    sortedCandidates,
    (candidates) => {
      // 候选刷新或筛选变化后尽量保留用户选择；原选项消失时再自动选中第一项。
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
    // 后端候选数量是 Binance 期权“张”口径，前端只承接默认值，真实校验仍在 Rust。
    form.quantity = decimalStringToDisplayNumber(candidate.quantity)
  }

  function resetCandidateFilters() {
    candidateFilter.minAnnualized = null
    candidateFilter.maxExerciseProbability = null
    candidateFilter.maxDistanceAbs = null
  }

  return {
    form,
    candidateFilter,
    selectedCandidate,
    filteredCandidates,
    sortedCandidates,
    expiryFilter,
    candidateSortKey,
    candidateSortDirection,
    referenceSpotPrice,
    panelTitle,
    emptyCandidateTitle,
    emptyCandidateHint,
    projectedPremium,
    projectedBreakeven,
    canSubmit,
    daysUntil,
    toggleCandidateSort,
    selectDirection,
    selectCandidate,
    resetCandidateFilters,
  }
}

/** 计算展示用剩余天数；真实到期判断以后端 expiryAt 和结算扫描为准。 */
export function daysUntil(expiry: string) {
  const target = expiry.includes('T') ? new Date(expiry) : new Date(`${expiry}T00:00:00`)
  return Math.max(0, Math.ceil((target.getTime() - Date.now()) / dayMs))
}

/** 根据页面到期筛选项判断候选是否可见。 */
function matchesExpiryFilter(candidate: StrategyCandidate, expiryFilter: ExpiryFilter) {
  const days = daysUntil(candidate.expiryAt)
  if (expiryFilter === 'lt3')
    return days < 3
  if (expiryFilter === 'lt7')
    return days < 7
  if (expiryFilter === 'd7_30')
    return days >= 7 && days <= 30
  if (expiryFilter === 'd30_60')
    return days > 30 && days <= 60
  if (expiryFilter === 'gt60')
    return days > 60
  return true
}

/** 应用前端轻量筛选；这些筛选不会放宽 Rust 已执行的实盘硬约束。 */
function matchesCandidateFilters(
  candidate: StrategyCandidate,
  candidateFilter: CandidateFilterForm,
) {
  const annualized = decimalStringToDisplayNumber(candidate.annualized)
  const exerciseProbability = decimalStringToDisplayNumber(candidate.exerciseProbability)
  const distanceAbs = Math.abs(decimalStringToDisplayNumber(candidate.strikeDistancePercent))

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

/** 读取排序字段的数值表示，非法值排到列表后面。 */
function candidateSortNumber(candidate: StrategyCandidate, key: CandidateSortKey) {
  if (key === 'expiry') {
    return new Date(candidate.expiryAt).getTime()
  }
  return decimalStringToDisplayNumber(candidate[key])
}

function compareCandidateSortValue(
  left: StrategyCandidate,
  right: StrategyCandidate,
  key: CandidateSortKey,
) {
  // 非法数值排在后面，但不丢弃候选，方便用户看到后端返回的异常数据形态。
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
