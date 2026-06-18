<!--
双币赢工作台。

统一承载候选浏览和手动下单；没有 mock 数据路径。
-->
<script setup lang="ts">
import type { StrategyCandidate } from '@/types/domain'

import {
  ArrowDownCircle,
  ArrowUpCircle,
  RefreshCw,
  Zap,
} from '@lucide/vue'
import {
  NAlert,
  NButton,
  NIcon,
  NInput,
  NInputNumber,
} from 'naive-ui'
import { computed, shallowRef } from 'vue'
import OrderProgressPanel from '@/components/common/OrderProgressPanel.vue'
import CandidateTable from '@/components/dual/CandidateTable.vue'
import OrderReviewDialog from '@/components/orders/OrderReviewDialog.vue'
import {
  expiryFilters,
  useDualInvestmentCandidates,
} from '@/composables/useDualInvestmentCandidates'
import { useFormat } from '@/composables/useFormat'
import { messageText, t } from '@/i18n'
import { useAppStore } from '@/stores/app'
import { localDateTimeLabel } from '@/utils/date'
import { isPositiveDecimalString } from '@/utils/decimal'

interface ManualOrderReview {
  candidate: StrategyCandidate
  quantity: string
  projectedPremium: string
  projectedBreakeven: string
  daysToExpiry: number
}

const store = useAppStore()
// 完整合约代码区分到期日、执行价和 Put/Call，避免把同价的其他合约也标成持仓。
const heldOptionSymbols = computed(() => new Set(store.openPositions.map(position => position.optionSymbol)))
const { money, percent, signedPercent } = useFormat()
const {
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
} = useDualInvestmentCandidates()
const orderReviewVisible = shallowRef(false)
const orderReview = shallowRef<ManualOrderReview>()

function submit() {
  const candidate = selectedCandidate.value
  const preview = strategyPreview.value
  if (!candidate || !preview || !canSubmit.value) {
    return
  }
  // 冻结用户点击发送时看到的候选快照，避免确认弹窗打开后候选刷新导致展示和实际下单不一致。
  orderReview.value = {
    candidate,
    quantity: form.quantity.trim(),
    projectedPremium: preview.premium,
    projectedBreakeven: preview.breakeven,
    daysToExpiry: daysUntil(candidate.expiryAt),
  }
  orderReviewVisible.value = true
}

function closeOrderReview() {
  orderReviewVisible.value = false
}

function clearOrderReviewSnapshot() {
  if (!orderReviewVisible.value) {
    orderReview.value = undefined
  }
}

function confirmOrderReview() {
  const review = orderReview.value
  if (!orderReviewVisible.value || !review) {
    return
  }
  closeOrderReview()
  const { candidate, quantity } = review
  // 手动下单也走真实 Binance 订单，只有复核弹窗确认后才传 `confirmLive: true`。
  void store.placeStrategy({
    symbol: candidate.symbol,
    optionSymbol: candidate.optionSymbol,
    direction: candidate.direction,
    expiryAt: candidate.expiryAt,
    strike: candidate.strike,
    quantity,
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
          <span> {{ t('ui.buyLow') }} </span>
          <strong> {{ t('ui.sellPut') }} </strong>
        </button>
        <button :class="{ active: form.direction === 'sellCall' }" type="button" @click="selectDirection('sellCall')">
          <NIcon :component="ArrowUpCircle" />
          <span> {{ t('ui.sellHigh') }} </span>
          <strong> {{ t('ui.sellCall') }} </strong>
        </button>
      </section>

      <section class="candidate-board">
        <div class="candidate-toolbar">
          <div class="expiry-tabs" :aria-label="t('ui.filterByExpiry')">
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
            <span v-if="referenceSpotPrice" class="spot-price"> {{ t('ui.btcSpot') }} {{ money(referenceSpotPrice) }}</span>
            <NButton
              circle
              secondary
              size="small"
              :title="t('ui.refreshCandidates')"
              :aria-label="t('ui.refreshCandidates')"
              :loading="store.isLoading('scan')"
              @click="store.scan()"
            >
              <template #icon><NIcon :component="RefreshCw" /></template>
            </NButton>
          </div>
        </div>

        <div class="candidate-filter-bar" :aria-label="t('ui.candidateFilters')">
          <label> {{ t('ui.annualized') }} <NInputNumber
            v-model:value="candidateFilter.minAnnualized"
            clearable
            :placeholder="t('ui.any')"
            :min="0"
            :step="1"
          />
          </label>
          <label> {{ t('ui.exerciseProbability2') }} <NInputNumber
            v-model:value="candidateFilter.maxExerciseProbability"
            clearable
            :placeholder="t('ui.any')"
            :min="0"
            :max="100"
            :step="1"
          />
          </label>
          <label> {{ t('ui.distanceFromSpot') }} <NInputNumber
            v-model:value="candidateFilter.maxDistanceAbs"
            clearable
            :placeholder="t('ui.any')"
            :min="0"
            :step="0.5"
          />
          </label>
          <NButton secondary @click="resetCandidateFilters"> {{ t('ui.reset') }} </NButton>
        </div>

        <CandidateTable
          :candidates="sortedCandidates"
          :held-option-symbols="heldOptionSymbols"
          :selected-option-symbol="selectedCandidate?.optionSymbol"
          :sort-key="candidateSortKey"
          :sort-direction="candidateSortDirection"
          @select="selectCandidate"
          @toggle-sort="toggleCandidateSort"
        />

        <div v-if="filteredCandidates.length === 0" class="empty-state opportunity-empty">
          <strong>{{ emptyCandidateTitle }}</strong>
          <span>{{ emptyCandidateHint }}</span>
          <NButton size="small" secondary :loading="store.isLoading('scan')" @click="store.scan()"> {{ t('ui.scanAgain') }} </NButton>
        </div>
      </section>
    </div>

    <aside class="order-panel">
      <section class="order-card">
        <header>
          <div>
            <span class="order-eyebrow"> {{ t('ui.placeOrder') }} </span>
            <h3>{{ panelTitle }}</h3>
          </div>
          <NIcon :component="Zap" />
        </header>

        <div class="deal-summary">
          <article>
            <span> {{ t('ui.annualizedReturn') }} </span>
            <strong :class="{ positive: selectedCandidate }">
              {{ selectedCandidate ? percent(selectedCandidate.annualized) : '--' }}
            </strong>
            <em v-if="selectedCandidate && isPositiveDecimalString(selectedCandidate.spotPrice)"> {{ t('ui.fromSpot') }} {{ signedPercent(selectedCandidate.strikeDistancePercent) }}
            </em>
          </article>
          <article>
            <span> {{ t('ui.expiryDate') }} </span>
            <strong>{{ selectedCandidate ? localDateTimeLabel(selectedCandidate.expiryAt) : '--' }}</strong>
            <em v-if="selectedCandidate">{{ daysUntil(selectedCandidate.expiryAt) }} {{ t('ui.days') }} </em>
          </article>
          <article>
            <span> {{ t('ui.estimatedPremium') }} </span>
            <strong>{{ strategyPreview ? money(strategyPreview.premium) : previewLoading ? t('ui.calculating') : '--' }}</strong>
          </article>
          <article>
            <span> {{ t('ui.breakevenPrice') }} </span>
            <strong>{{ strategyPreview ? money(strategyPreview.breakeven, 'USDT', 0) : previewLoading ? t('ui.calculating') : '--' }}</strong>
          </article>
        </div>

        <div class="order-form">
          <label> {{ t('ui.quantityContracts') }} <NInput v-model:value="form.quantity" :input-props="{ inputmode: 'decimal' }" :placeholder="t('ui.atLeast')" />
          </label>
        </div>

        <NAlert v-if="previewError" type="error">{{ messageText(previewError) }}</NAlert>

        <NButton type="primary" size="large" block :disabled="!canSubmit" :loading="store.isLoading('order')" @click="submit">
          <template #icon><NIcon :component="Zap" /></template> {{ t('ui.submitOrder') }}
        </NButton>

        <OrderProgressPanel
          :progress="store.orderProgress"
          @close="store.clearOrderProgress"
          @view-orders="store.openView('orders')"
        />
      </section>
    </aside>

    <OrderReviewDialog
      :show="orderReviewVisible"
      :candidate="orderReview?.candidate"
      :quantity="orderReview?.quantity ?? '0'"
      :projected-premium="orderReview?.projectedPremium ?? '0'"
      :projected-breakeven="orderReview?.projectedBreakeven ?? '0'"
      :days-to-expiry="orderReview?.daysToExpiry ?? 0"
      @close="closeOrderReview"
      @confirm="confirmOrderReview"
      @after-leave="clearOrderReviewSnapshot"
    />
  </div>
</template>
