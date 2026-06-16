// 双币赢候选列表的前端交互状态。
//
// Rust 策略层已经完成实盘合约过滤和方向硬约束；这个 composable 只负责
// 页面上的方向切换、到期/年化筛选、稳定排序、默认选中和下单卡片的预览状态。

import type { LocalizedMessage } from '@/i18n'
import type { StrategyCandidate, StrategyDirection, StrategyPreview } from '@/types/domain'
import { computed, reactive, shallowRef, watch } from 'vue'
import { t } from '@/i18n'
import { api, commandError } from '@/services/api'
import { useAppStore } from '@/stores/app'
import { compareDecimalStrings, isPositiveDecimalString } from '@/utils/decimal'

interface CandidateFilterForm {
  minAnnualized: number | null
  maxExerciseProbability: number | null
  maxDistanceAbs: number | null
}

export type CandidateSortKey = 'strike' | 'annualized' | 'expiry' | 'limitPrice' | 'exerciseProbability'
export type CandidateSortDirection = 'asc' | 'desc'

// 剩余时长按 24 小时向上取整为天；实际到期和下单请求仍使用 Rust 返回的精确 expiryAt。
const dayMs = 86_400_000

export const expiryFilters = [
  { id: 'all', get label() { return t('ui.allExpiries') } },
  { id: 'lt3', get label() { return t('ui.days3') } },
  { id: 'lt7', get label() { return t('ui.days4') } },
  { id: 'd7_30', get label() { return t('ui.days5') } },
  { id: 'd30_60', get label() { return t('ui.days6') } },
  { id: 'gt60', get label() { return t('ui.days7') } },
] as const

export const candidateSortColumns: Array<{ key: CandidateSortKey, label: string }> = [
  { key: 'strike', get label() { return t('ui.targetPrice') } },
  { key: 'annualized', get label() { return t('ui.annualizedReturn') } },
  { key: 'expiry', get label() { return t('ui.expiry') } },
  { key: 'limitPrice', get label() { return t('ui.currentPrice') } },
  { key: 'exerciseProbability', get label() { return t('ui.exerciseProbability') } },
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
  const selectedOptionSymbol = shallowRef<string>()
  const expiryFilter = shallowRef<ExpiryFilter>('all')
  const candidateSortKey = shallowRef<CandidateSortKey>('annualized')
  const candidateSortDirection = shallowRef<CandidateSortDirection>('desc')

  const form = reactive({
    direction: 'sellPut' as StrategyDirection,
    quantity: '0.01',
  })

  const candidateFilter = reactive<CandidateFilterForm>({
    minAnnualized: null,
    maxExerciseProbability: null,
    maxDistanceAbs: null,
  })

  const selectedCandidate = computed(() =>
    store.candidates.find(candidate => candidate.optionSymbol === selectedOptionSymbol.value),
  )

  const filteredCandidates = computed(() => {
    // 前端筛选只影响展示。候选生成和方向/现货价硬约束已经在 Rust 策略层完成。
    return store.candidates
      .filter(candidate => candidate.direction === form.direction)
      .filter(candidate => matchesExpiryFilter(candidate, expiryFilter.value))
      .filter(candidate => matchesCandidateFilters(candidate, candidateFilter))
  })

  const sortedCandidates = computed(() => {
    const modifier = candidateSortDirection.value === 'asc' ? 1 : -1
    // 原生稳定排序保留同值候选的扫描顺序，复制数组以免改变筛选结果。
    return [...filteredCandidates.value].sort((left, right) =>
      compareCandidateSortValue(left, right, candidateSortKey.value) * modifier,
    )
  })

  const activeCandidateFilterCount = computed(() =>
    [
      candidateFilter.minAnnualized,
      candidateFilter.maxExerciseProbability,
      candidateFilter.maxDistanceAbs,
    ].filter(value => value !== null).length,
  )

  const referenceSpotPrice = computed(() =>
    store.candidates.find(candidate => isPositiveDecimalString(candidate.spotPrice))?.spotPrice,
  )

  const directionStats = computed(() => ({
    sellPut: store.candidates.filter(candidate => candidate.direction === 'sellPut').length,
    sellCall: store.candidates.filter(candidate => candidate.direction === 'sellCall').length,
  }))

  const panelTitle = computed(() =>
    form.direction === 'sellPut' ? t('ui.buyBtcLowWithUsdt') : t('ui.sellBtcHigh'),
  )

  const activeDirectionLabel = computed(() =>
    form.direction === 'sellPut' ? t('ui.buyLow') : t('ui.sellHigh'),
  )

  const otherDirectionLabel = computed(() =>
    form.direction === 'sellPut' ? t('ui.sellHigh') : t('ui.buyLow'),
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
      return t('ui.noCandidates')
    if (activeCandidateFilterCount.value > 0)
      return t('ui.noCandidatesMatchTheseFilters')
    if (currentDirectionCandidateCount.value > 0)
      return t('ui.noCandidatesForThisExpiry')
    return t('ui.noCandidates2', { p0: activeDirectionLabel.value, p1: activeOptionType.value })
  })

  const emptyCandidateHint = computed(() => {
    if (store.candidates.length === 0)
      return t('ui.noCandidatesYetRunANewScan')
    if (activeCandidateFilterCount.value > 0) {
      return t('ui.relaxTheAnnualizedReturnExerciseProbabilityOr')
    }
    if (currentDirectionCandidateCount.value === 0 && otherDirectionCandidateCount.value > 0) {
      return t('ui.thisScanOnlyFoundCandidatesNoCandidates', { p0: otherDirectionLabel.value, p1: activeDirectionLabel.value, p2: otherDirectionLabel.value })
    }
    return t('ui.noMatchesForThisExpiryFilterCandidates')
  })

  const strategyPreview = shallowRef<StrategyPreview>()
  const previewLoading = shallowRef(false)
  const previewError = shallowRef<LocalizedMessage>()

  // 输入变化时立即清空预览并使旧请求失效；卸载时 Vue 同样执行清理。
  watch([
    () => selectedCandidate.value?.direction,
    () => selectedCandidate.value?.strike,
    () => selectedCandidate.value?.limitPrice,
    () => form.quantity.trim(),
  ], async ([direction, strike, limitPrice, quantity], _previous, onCleanup) => {
    strategyPreview.value = undefined
    previewError.value = undefined
    previewLoading.value = false
    if (!direction || !strike || !limitPrice || !quantity) {
      return
    }
    let active = true
    onCleanup(() => {
      active = false
    })
    previewLoading.value = true
    try {
      const value = await api.previewStrategy({
        direction,
        strike,
        limitPrice,
        quantity,
      })
      if (active) {
        strategyPreview.value = value
      }
    }
    catch (caught) {
      if (active) {
        previewError.value = commandError(caught).message
      }
    }
    finally {
      if (active) {
        previewLoading.value = false
      }
    }
  }, { immediate: true, flush: 'sync' })

  const canSubmit = computed(() => {
    // 前端只做基本可用性判断，真实合约精度、保证金和风控仍在 Rust 下单链路校验。
    return Boolean(store.settings?.connected)
      && Boolean(selectedCandidate.value)
      && Boolean(strategyPreview.value)
  })

  watch(
    sortedCandidates,
    (candidates) => {
      // 候选刷新或筛选变化后尽量保留用户选择；原选项消失时再自动选中第一项。
      if (selectedOptionSymbol.value && candidates.some(candidate => candidate.optionSymbol === selectedOptionSymbol.value)) {
        return
      }
      if (candidates[0]) {
        selectCandidate(candidates[0])
        return
      }
      selectedOptionSymbol.value = undefined
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
    selectedOptionSymbol.value = undefined
  }

  function selectCandidate(candidate: StrategyCandidate) {
    selectedOptionSymbol.value = candidate.optionSymbol
    form.direction = candidate.direction
    // 后端候选数量是 Binance 期权“张”口径，前端只承接默认值，真实校验仍在 Rust。
    form.quantity = candidate.quantity
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
    strategyPreview,
    previewLoading,
    previewError,
    canSubmit,
    daysUntil,
    toggleCandidateSort,
    selectDirection,
    selectCandidate,
    resetCandidateFilters,
  }
}

/** 计算展示用剩余天数；真实到期判断以后端 expiryAt 和结算扫描为准。 */
export function daysUntil(expiryAt: string) {
  const target = new Date(expiryAt)
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
  if (candidateFilter.minAnnualized !== null
    && compareDecimalStrings(candidate.annualized, String(candidateFilter.minAnnualized)) === -1) {
    return false
  }
  if (
    candidateFilter.maxExerciseProbability !== null
    && compareDecimalStrings(candidate.exerciseProbability, String(candidateFilter.maxExerciseProbability)) === 1
  ) {
    return false
  }
  const distanceAbs = candidate.strikeDistancePercent.replace(/^[+-]/, '')
  if (candidateFilter.maxDistanceAbs !== null
    && compareDecimalStrings(distanceAbs, String(candidateFilter.maxDistanceAbs)) === 1) {
    return false
  }
  return true
}

function compareCandidateSortValue(
  left: StrategyCandidate,
  right: StrategyCandidate,
  key: CandidateSortKey,
) {
  if (key === 'expiry') {
    return new Date(left.expiryAt).getTime() - new Date(right.expiryAt).getTime()
  }
  return compareDecimalStrings(left[key], right[key]) ?? 0
}
