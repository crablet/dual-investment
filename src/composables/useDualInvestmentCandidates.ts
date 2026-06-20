// 候选合约的筛选、排序、选择和订单预览。合约交易条件由 Rust 检查。

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
    // 这里筛选已通过 Rust 检查的候选，只改变显示结果。
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
    // 连接成功、已选合约且预估完成后才可提交；Rust 提交时再校验合约和账户。
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
    // 数量单位为合约张数，默认值来自后端候选。
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

/** 显示剩余天数；交割仍按后端精确到期时间执行。 */
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

/** 按年化、行权概率和距现货幅度筛选候选。 */
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
