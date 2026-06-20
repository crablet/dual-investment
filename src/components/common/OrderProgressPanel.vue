<!-- 展示下单进度事件；确认订单结果时仍需读取后端快照。 -->
<script setup lang="ts">
import type { OrderProgressStepView, OrderProgressView } from '@/types/view'

import {
  AlertTriangle,
  CheckCircle2,
  Circle,
  Loader2,
  X,
  XCircle,
} from '@lucide/vue'
import { NButton, NIcon, NTag } from 'naive-ui'
import { computed } from 'vue'
import { useFormat } from '@/composables/useFormat'
import { messageText, t } from '@/i18n'

const props = defineProps<{
  progress?: OrderProgressView
}>()

const emit = defineEmits<{
  close: []
  viewOrders: []
}>()

const { money } = useFormat()

const statusText = computed(() => {
  if (!props.progress)
    return t('ui.readyToSubmit')
  if (props.progress.status === 'success')
    return t('ui.completed')
  if (props.progress.status === 'error')
    return t('ui.failed')
  return t('ui.inProgress')
})

const statusTone = computed(() => {
  if (props.progress?.status === 'success')
    return 'success'
  if (props.progress?.status === 'error')
    return 'error'
  return 'warning'
})

function stepIcon(step: OrderProgressStepView) {
  if (step.status === 'success')
    return CheckCircle2
  if (step.status === 'error')
    return XCircle
  if (step.status === 'running')
    return Loader2
  return Circle
}
</script>

<template>
  <section v-if="progress" class="order-progress-panel" role="status" aria-live="polite">
    <header>
      <div>
        <span> {{ t('ui.orderProgress') }} </span>
        <strong>{{ progress.optionSymbol ?? t('ui.waitingForContract') }}</strong>
      </div>
      <div class="order-progress-actions">
        <NTag :type="statusTone" size="small">{{ statusText }}</NTag>
        <NButton size="tiny" quaternary circle :title="t('ui.closeProgress')" @click="emit('close')">
          <template #icon><NIcon aria-hidden="true" :component="X" /></template>
        </NButton>
      </div>
    </header>

    <ol class="order-progress-steps">
      <li
        v-for="step in progress.steps"
        :key="step.id"
        :class="[`status-${step.status}`]"
      >
        <NIcon aria-hidden="true" :component="stepIcon(step)" />
        <div>
          <strong>{{ messageText(step.label) }}</strong>
          <span>{{ messageText(step.message) || t('ui.waiting') }}</span>
        </div>
      </li>
    </ol>

    <div v-if="progress.status === 'success' && progress.order" class="order-progress-result success">
      <strong> {{ t('ui.orderCreated') }} </strong>
      <span> {{ t('ui.filled') }} {{ progress.order.filledQuantity }} / {{ progress.order.quantity }} {{ t('ui.contractsAveragePrice') }} {{ money(progress.order.avgPrice) }} {{ t('ui.premium') }} {{ money(progress.order.premium) }}
      </span>
    </div>

    <div v-else-if="progress.status === 'error'" class="order-progress-result error">
      <NIcon aria-hidden="true" :component="AlertTriangle" />
      <span>{{ messageText(progress.error) || t('ui.orderFailedCheckTheErrorAndRecovery') }}</span>
    </div>

    <div class="order-progress-footer">
      <span v-if="progress.exchangeOrderId"> {{ t('ui.exchangeOrderId') }} {{ progress.exchangeOrderId }}</span>
      <NButton size="small" secondary @click="emit('viewOrders')"> {{ t('ui.viewStrategyOrders') }} </NButton>
    </div>
  </section>
</template>
