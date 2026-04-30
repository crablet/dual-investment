<!--
双币赢工作台。

统一承载候选浏览、手动下单和自动复投规则；没有独立自动复投页面或 mock 数据路径。
-->
<script setup lang="ts">
import type { StrategyCandidate } from '@/types/domain'
import {
  ArrowDownCircle,
  ArrowUpCircle,
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
import { shallowRef } from 'vue'
import OrderProgressPanel from '@/components/common/OrderProgressPanel.vue'
import CandidateTable from '@/components/dual/CandidateTable.vue'
import OrderReviewDialog from '@/components/orders/OrderReviewDialog.vue'
import { useAutoRuleForm } from '@/composables/useAutoRuleForm'
import {
  expiryFilters,
  useDualInvestmentCandidates,
} from '@/composables/useDualInvestmentCandidates'
import { useFormat } from '@/composables/useFormat'
import { useAppStore } from '@/stores/app'
import { localDateTimeLabel } from '@/utils/date'
import { decimalStringToDisplayNumber } from '@/utils/decimal'

interface ManualOrderReview {
  candidate: StrategyCandidate
  quantity: string
  projectedPremium: string
  projectedBreakeven: string
  daysToExpiry: number
}

const store = useAppStore()
const { money, percent } = useFormat()
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
  projectedPremium,
  projectedBreakeven,
  canSubmit,
  daysUntil,
  toggleCandidateSort,
  selectDirection,
  selectCandidate,
  resetCandidateFilters,
} = useDualInvestmentCandidates()
const {
  advancedAutoOpen,
  ruleForm,
  saveAutoRule,
  toggleAuto,
  runAutoNow,
} = useAutoRuleForm()
const orderReviewVisible = shallowRef(false)
const orderReview = shallowRef<ManualOrderReview>()

function signedPercent(value: string | number) {
  // 展示用百分比允许 number；真实年化和风控阈值提交前会回到字符串契约。
  const numeric = typeof value === 'number' ? value : decimalStringToDisplayNumber(value)
  if (!Number.isFinite(numeric))
    return '--'
  const sign = numeric > 0 ? '+' : ''
  return `${sign}${percent(numeric)}`
}

function submit() {
  const candidate = selectedCandidate.value
  if (!candidate) {
    return
  }
  // 冻结用户点击发送时看到的候选快照，避免确认弹窗打开后候选刷新导致展示和实际下单不一致。
  orderReview.value = {
    candidate,
    quantity: form.quantity.toFixed(2),
    projectedPremium: String(projectedPremium.value),
    projectedBreakeven: String(projectedBreakeven.value),
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
  if (!review) {
    return
  }
  closeOrderReview()
  const { candidate, quantity } = review
  // 手动下单也走真实 Binance 订单，只有复核弹窗确认后才传 `confirmLive: true`。
  void store.placeStrategy({
    candidateId: candidate.id,
    symbol: candidate.symbol,
    optionSymbol: candidate.optionSymbol,
    direction: candidate.direction,
    expiry: candidate.expiry,
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

        <CandidateTable
          :candidates="sortedCandidates"
          :selected-candidate-id="selectedCandidate?.id"
          :sort-key="candidateSortKey"
          :sort-direction="candidateSortDirection"
          @select="selectCandidate"
          @toggle-sort="toggleCandidateSort"
        />

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
            <em v-if="selectedCandidate && decimalStringToDisplayNumber(selectedCandidate.spotPrice) > 0">
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
