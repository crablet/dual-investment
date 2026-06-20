<!-- 显示复核时固定的合约与数量，用户确认后由父页面提交。
Rust 提交前会重新查询买一价和数量，更新下单限价。 -->
<script setup lang="ts">
import type { StrategyCandidate } from '@/types/domain'

import { X } from '@lucide/vue'
import {
  NButton,
  NIcon,
  NModal,
} from 'naive-ui'
import { computed } from 'vue'
import { useFormat } from '@/composables/useFormat'
import { t } from '@/i18n'
import { localDateTimeLabel } from '@/utils/date'

interface Props {
  show: boolean
  candidate?: StrategyCandidate
  quantity: string
  projectedPremium: string
  projectedBreakeven: string
  daysToExpiry: number
}

interface ReviewFact {
  label: string
  value: string
  tone?: 'gold' | 'green'
}

const props = defineProps<Props>()
const emit = defineEmits<{
  close: []
  confirm: []
  afterLeave: []
}>()

const { money, percent, signedPercent } = useFormat()

const isSellPut = computed(() => props.candidate?.direction === 'sellPut')
const productType = computed(() => isSellPut.value ? t('ui.buyLowSellPut') : t('ui.sellHighSellCall'))
const tradingPair = computed(() => isSellPut.value ? t('ui.buyBtcLowWithUsdt') : t('ui.sellBtcHigh'))
const reviewFacts = computed<ReviewFact[]>(() => {
  if (!props.candidate) {
    return []
  }
  return [
    { label: t('ui.productType'), value: productType.value, tone: 'gold' },
    { label: t('ui.tradingPair'), value: tradingPair.value },
    { label: t('ui.annualizedReturn'), value: percent(props.candidate.annualized), tone: 'green' },
    {
      label: t('ui.expiryDate'),
      value: t('ui.days2', { p0: localDateTimeLabel(props.candidate.expiryAt), p1: props.daysToExpiry }),
    },
    { label: t('ui.orderQuantity'), value: t('ui.contracts', { p0: props.quantity }) },
    { label: t('ui.estimatedPremium'), value: money(props.projectedPremium) },
    { label: t('ui.strikePrice'), value: money(props.candidate.strike, 'USDT', 0) },
    { label: t('ui.breakevenPrice'), value: money(props.projectedBreakeven, 'USDT', 0) },
    { label: t('ui.currentPrice'), value: money(props.candidate.limitPrice, 'USDT', 0) },
    { label: t('ui.exerciseProbability'), value: percent(props.candidate.exerciseProbability) },
    { label: t('ui.fromSpot'), value: signedPercent(props.candidate.strikeDistancePercent) },
  ]
})

function confirm() {
  if (props.show && props.candidate)
    emit('confirm')
}
</script>

<template>
  <NModal
    :show="props.show"
    :mask-closable="false"
    :close-on-esc="false"
    :auto-focus="false"
    @after-leave="emit('afterLeave')"
  >
    <section
      class="order-review-dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="order-review-title"
    >
      <button class="order-review-close" type="button" :aria-label="t('ui.closeOrderReview')" @click="emit('close')">
        <NIcon aria-hidden="true" :component="X" />
      </button>

      <header class="order-review-header">
        <h2 id="order-review-title"> {{ t('ui.confirmOrder') }} </h2>
      </header>

      <dl class="order-review-facts">
        <div v-for="fact in reviewFacts" :key="fact.label" class="order-review-fact">
          <dt>{{ fact.label }}</dt>
          <dd :class="fact.tone">{{ fact.value }}</dd>
        </div>
      </dl>

      <footer class="order-review-actions">
        <NButton size="large" secondary class="order-review-back" @click="emit('close')"> {{ t('ui.backToEdit') }} </NButton>
        <NButton size="large" type="primary" class="order-review-confirm" :disabled="!props.show" @click="confirm"> {{ t('ui.confirmSubmission') }} </NButton>
      </footer>
    </section>
  </NModal>
</template>

<style scoped>
.order-review-dialog {
  position: relative;
  width: min(640px, calc(100vw - 48px));
  max-height: calc(100vh - 48px);
  overflow: auto;
  padding: 28px 32px;
  border: 1px solid var(--line);
  border-radius: 10px;
  background: var(--surface);
  box-shadow: 0 20px 64px rgba(0, 0, 0, 0.42);
}

.order-review-close {
  position: absolute;
  top: 18px;
  right: 18px;
  display: grid;
  place-items: center;
  width: 30px;
  height: 30px;
  padding: 0;
  border: 0;
  color: #d7dee8;
  background: transparent;
  cursor: pointer;
}

.order-review-close:hover {
  color: var(--text);
}

.order-review-close .n-icon {
  font-size: 22px;
}

.order-review-header {
  padding-right: 36px;
  margin-bottom: 18px;
}

.order-review-header h2 {
  margin: 0;
  font-size: 28px;
  line-height: 1.2;
  letter-spacing: -0.03em;
  color: #f4f7fb;
}

.order-review-facts {
  display: grid;
  gap: 13px;
  margin: 0;
  padding: 16px 20px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.025);
}

.order-review-fact {
  display: grid;
  grid-template-columns: 130px minmax(0, 1fr);
  gap: 18px;
  align-items: baseline;
}

.order-review-fact dt {
  color: #aeb8c5;
  font-size: 15px;
}

.order-review-fact dd {
  margin: 0;
  overflow-wrap: anywhere;
  color: #f4f7fb;
  font-size: 15px;
  font-weight: 700;
}

.order-review-fact dd.gold {
  color: var(--gold);
}

.order-review-fact dd.green {
  color: var(--green);
}

.order-review-actions {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 18px;
  justify-content: end;
  width: min(370px, 100%);
  margin: 22px 0 0 auto;
}

.order-review-back,
.order-review-confirm {
  min-height: 48px;
  font-size: 16px;
}

.order-review-confirm:not(.n-button--disabled) {
  color: #1b1400;
  font-weight: 800;
}

@media (max-width: 1180px) {
  .order-review-dialog {
    width: min(620px, calc(100vw - 36px));
    padding: 24px 28px;
  }
}

@media (max-width: 720px) {
  .order-review-fact {
    grid-template-columns: 1fr;
    gap: 2px;
  }

  .order-review-actions {
    grid-template-columns: 1fr;
  }
}
</style>
