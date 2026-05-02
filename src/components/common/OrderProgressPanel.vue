<!--
手动下单进度面板。

只展示前端接收到的脱敏阶段事件；真实订单事实仍以策略订单列表和后端快照为准。
-->
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
    return '待下单'
  if (props.progress.status === 'success')
    return '已完成'
  if (props.progress.status === 'error')
    return '失败'
  return '进行中'
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
        <span>下单进度</span>
        <strong>{{ progress.optionSymbol ?? '等待合约' }}</strong>
      </div>
      <div class="order-progress-actions">
        <NTag :type="statusTone" size="small">{{ statusText }}</NTag>
        <NButton size="tiny" quaternary circle title="关闭进度" @click="emit('close')">
          <template #icon><NIcon :component="X" /></template>
        </NButton>
      </div>
    </header>

    <ol class="order-progress-steps">
      <li
        v-for="step in progress.steps"
        :key="step.id"
        :class="[`status-${step.status}`]"
      >
        <NIcon :component="stepIcon(step)" />
        <div>
          <strong>{{ step.label }}</strong>
          <span>{{ step.message ?? '等待执行' }}</span>
        </div>
      </li>
    </ol>

    <div v-if="progress.status === 'success' && progress.order" class="order-progress-result success">
      <strong>订单已创建</strong>
      <span>
        成交 {{ progress.order.filledQuantity }} / {{ progress.order.quantity }} 张，
        均价 {{ money(progress.order.avgPrice) }}，权利金 {{ money(progress.order.premium) }}
      </span>
    </div>

    <div v-else-if="progress.status === 'error'" class="order-progress-result error">
      <NIcon :component="AlertTriangle" />
      <span>{{ progress.error ?? '下单失败，请查看错误提示和恢复记录。' }}</span>
    </div>

    <div class="order-progress-footer">
      <span v-if="progress.exchangeOrderId">交易所订单号 {{ progress.exchangeOrderId }}</span>
      <NButton size="small" secondary @click="emit('viewOrders')">查看策略订单</NButton>
    </div>
  </section>
</template>
