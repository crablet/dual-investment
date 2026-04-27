<script setup lang="ts">
import type {
  FundingAttemptManualResolution,
  FundingAttemptView,
  OrderAttemptView,
  StrategyOrder,
} from '@/types/domain'
// 策略订单视图。
// 展示真实订单、同步/结算操作和恢复记录；恢复动作会回到 Rust 重新查询 Binance。
import { CheckCircle2, RefreshCw, ScanLine, XCircle } from '@lucide/vue'
import { NButton, NIcon, NTag } from 'naive-ui'
import { computed, shallowRef, watchEffect } from 'vue'
import Panel from '@/components/common/Panel.vue'
import AdoptOptionPositionsPanel from '@/components/orders/AdoptOptionPositionsPanel.vue'
import { useFormat } from '@/composables/useFormat'
import { useAppStore } from '@/stores/app'
import { localDateKey, localDateTimeLabel } from '@/utils/date'
import { attemptStatusText, fundingAttemptStatusText, recoveryStatusTone } from '@/utils/labels'

const store = useAppStore()
const { directionText, directionTone, money, statusText, statusTone } = useFormat()
const selectedId = shallowRef<string>()

interface OrderDetailRow {
  label: string
  value: string
}

interface RecoveryRow {
  id: string
  source: 'order' | 'funding'
  updatedAt: string
  type: string
  target: string
  amount: string
  exchangeRef: string
  status: string
  factSource: string
  nextAction: string
  message: string
}

watchEffect(() => {
  // 首次进入或订单列表刷新后，默认选中最新订单，保留用户已有选择。
  if (!selectedId.value && store.orders[0]) {
    selectedId.value = store.orders[0].id
  }
})

const selected = computed(() => store.orders.find(order => order.id === selectedId.value) ?? store.orders[0])
const totalPremium = computed(() =>
  store.orders.reduce((sum, order) => sum + Number(order.premium), 0),
)
const todayExpiryCount = computed(() => {
  const today = localDateKey(new Date().toISOString())
  return store.orders.filter(order => localDateKey(order.expiryAt) === today).length
})
const recoveryRows = computed(() => [
  ...store.orderAttempts.map(orderAttemptRecoveryRow),
  ...store.fundingAttempts.map(fundingAttemptRecoveryRow),
].sort((left, right) => right.updatedAt.localeCompare(left.updatedAt)).slice(0, 10))
const detailRows = computed(() => {
  if (!selected.value) {
    return []
  }
  return createOrderDetailRows(selected.value)
})

function expiryLabel(order: { expiryAt: string }) {
  return localDateTimeLabel(order.expiryAt)
}

function createOrderDetailRows(order: StrategyOrder): OrderDetailRow[] {
  return [
    { label: '方向', value: directionText(order.direction) },
    { label: '标的', value: order.symbol },
    { label: '期权合约', value: order.optionSymbol },
    { label: '交易所状态', value: order.exchangeStatus },
    { label: '客户端订单号', value: order.clientOrderId },
    { label: '到期时间', value: expiryLabel(order) },
    { label: '执行价', value: money(order.strike) },
    { label: '成交数量', value: `${order.filledQuantity} / ${order.quantity} 张` },
    { label: '成交均价', value: money(order.avgPrice) },
    { label: '手续费', value: money(order.fee) },
    { label: '权利金', value: money(order.premium) },
    { label: '保本价', value: money(order.breakeven) },
    { label: '结算参考价', value: money(order.settlementPrice) },
    { label: '现货订单号', value: order.spotOrderId ?? '--' },
    { label: '现货状态', value: order.spotOrderStatus || '--' },
    { label: '现货成交数量', value: `${order.spotFilledQuantity} BTC` },
    { label: '现货成交均价', value: money(order.spotAvgPrice) },
    { label: '现货手续费', value: order.spotFee },
    { label: '到期补齐', value: order.settlementAction },
    { label: '补齐结果', value: order.rebalanceMessage || '--' },
  ]
}

function orderAttemptRecoveryRow(attempt: OrderAttemptView): RecoveryRow {
  return {
    id: attempt.id,
    source: 'order',
    updatedAt: attempt.updatedAt,
    type: attempt.status.startsWith('spot_') ? '现货补齐' : '期权订单',
    target: attempt.optionSymbol,
    amount: attempt.symbol,
    exchangeRef: attempt.exchangeOrderId ?? attempt.clientOrderId,
    status: attempt.status,
    factSource: attempt.exchangeOrderId ? `Binance 订单 ${attempt.exchangeOrderId}` : `客户端订单 ${attempt.clientOrderId}`,
    nextAction: attempt.status === 'persisted' || attempt.status === 'spot_persisted'
      ? '无需处理'
      : '重新查询 Binance 订单',
    message: attempt.message,
  }
}

function fundingAttemptRecoveryRow(attempt: FundingAttemptView): RecoveryRow {
  return {
    id: attempt.id,
    source: 'funding',
    updatedAt: attempt.updatedAt,
    type: attempt.kindLabel,
    target: attempt.transferType ?? attempt.productId ?? attempt.asset,
    amount: `${attempt.amount} ${attempt.asset}`,
    exchangeRef: attempt.exchangeRef ?? '--',
    status: attempt.status,
    factSource: attempt.factSource,
    nextAction: attempt.nextAction,
    message: attempt.message,
  }
}

function recoveryTimeLabel(value: string) {
  return value.slice(0, 19).replace('T', ' ')
}

function recoveryStatusLabel(row: RecoveryRow) {
  return row.source === 'funding'
    ? fundingAttemptStatusText(row.status)
    : attemptStatusText(row.status)
}

function canRecover(row: RecoveryRow) {
  return !['persisted', 'spot_persisted', 'confirmed', 'failed'].includes(row.status)
}

function canManuallyResolve(row: RecoveryRow) {
  return row.source === 'funding' && row.status === 'blocked'
}

function recoverActionLabel(row: RecoveryRow) {
  return row.status === 'blocked' ? '重查' : '恢复'
}

async function recoverRow(row: RecoveryRow) {
  if (row.source === 'order') {
    await store.recoverAttempt(row.id)
    return
  }
  await store.recoverFundingAttempt(row.id)
}

async function resolveFundingRow(row: RecoveryRow, resolution: FundingAttemptManualResolution) {
  const label = resolution === 'confirmed' ? '已在 Binance 核实该资金动作成功' : '已在 Binance 核实该资金动作失败或未发生'
  if (!window.confirm(`确认${label}？该操作只解除恢复阻断，不会重新发送资金请求。`)) {
    return
  }
  await store.resolveFundingAttempt(row.id, resolution)
}
</script>

<template>
  <div class="view-grid">
    <Panel title="订单统计" class="span-12">
      <template #action>
        <div class="button-row compact">
          <NButton size="small" secondary :loading="store.isLoading('sync')" @click="store.syncLiveState()">
            <template #icon><NIcon :component="RefreshCw" /></template>
            同步订单
          </NButton>
          <NButton size="small" secondary :loading="store.isLoading('settlement')" @click="store.runSettlementScan()">
            <template #icon><NIcon :component="ScanLine" /></template>
            结算扫描
          </NButton>
        </div>
      </template>
      <div class="summary-cards order-summary-cards">
        <article><span>总订单数</span><strong>{{ store.orders.length }}</strong></article>
        <article><span>进行中订单</span><strong>{{ store.runningOrders.length }}</strong></article>
        <article><span>今日到期</span><strong>{{ todayExpiryCount }}</strong></article>
        <article><span>累计权利金</span><strong>{{ money(String(totalPremium)) }}</strong></article>
      </div>
    </Panel>

    <Panel title="订单列表" class="span-8">
      <div class="table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th>订单编号</th>
              <th>方向</th>
              <th>标的</th>
              <th>执行价</th>
              <th>到期日</th>
              <th>数量</th>
              <th>权利金</th>
              <th>状态</th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="order in store.orders"
              :key="order.id"
              :class="{ selected: order.id === selectedId }"
              @click="selectedId = order.id"
            >
              <td>{{ order.id }}</td>
              <td><NTag :type="directionTone(order.direction)" size="small">{{ directionText(order.direction) }}</NTag></td>
              <td>{{ order.symbol }}</td>
              <td>{{ money(order.strike) }}</td>
              <td>{{ expiryLabel(order) }}</td>
              <td>{{ order.filledQuantity }} / {{ order.quantity }}</td>
              <td>{{ money(order.premium) }}</td>
              <td><NTag :type="statusTone(order.status)" size="small">{{ statusText(order.status) }}</NTag></td>
            </tr>
          </tbody>
        </table>
        <p v-if="store.orders.length === 0" class="empty-state">暂无订单</p>
      </div>
    </Panel>

    <Panel title="订单详情" class="span-4">
      <div v-if="selected" class="detail-list order-detail-list">
        <header class="order-detail-heading">
          <h3 :title="selected.id">{{ selected.id }}</h3>
          <NTag :type="statusTone(selected.status)" size="small" class="detail-status-tag">
            {{ statusText(selected.status) }}
          </NTag>
        </header>
        <p v-for="row in detailRows" :key="row.label">
          <span>{{ row.label }}</span>
          <strong :title="row.value">{{ row.value }}</strong>
        </p>
      </div>
      <p v-else class="empty-state">暂无订单详情</p>
    </Panel>

    <AdoptOptionPositionsPanel class="span-12" />

    <Panel title="恢复中心" class="span-12">
      <div class="table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th>时间</th>
              <th>类型</th>
              <th>对象</th>
              <th>金额/标的</th>
              <th>交易所引用</th>
              <th>状态</th>
              <th>事实来源</th>
              <th>下一步</th>
              <th>说明</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in recoveryRows" :key="`${row.source}-${row.id}`">
              <td>{{ recoveryTimeLabel(row.updatedAt) }}</td>
              <td>{{ row.type }}</td>
              <td>{{ row.target }}</td>
              <td>{{ row.amount }}</td>
              <td>{{ row.exchangeRef }}</td>
              <td>
                <NTag :type="recoveryStatusTone(row.status)" size="small">
                  {{ recoveryStatusLabel(row) }}
                </NTag>
              </td>
              <td>{{ row.factSource }}</td>
              <td>{{ row.nextAction }}</td>
              <td>{{ row.message }}</td>
              <td>
                <div class="button-row compact">
                  <NButton
                    size="small"
                    secondary
                    :loading="store.isLoading('recover')"
                    :disabled="!canRecover(row)"
                    @click="recoverRow(row)"
                  >
                    <template #icon><NIcon :component="RefreshCw" /></template>
                    {{ recoverActionLabel(row) }}
                  </NButton>
                  <template v-if="canManuallyResolve(row)">
                    <NButton
                      size="small"
                      type="success"
                      secondary
                      :loading="store.isLoading('recover')"
                      @click="resolveFundingRow(row, 'confirmed')"
                    >
                      <template #icon><NIcon :component="CheckCircle2" /></template>
                      核实成功
                    </NButton>
                    <NButton
                      size="small"
                      type="error"
                      secondary
                      :loading="store.isLoading('recover')"
                      @click="resolveFundingRow(row, 'failed')"
                    >
                      <template #icon><NIcon :component="XCircle" /></template>
                      核实失败
                    </NButton>
                  </template>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
        <p v-if="recoveryRows.length === 0" class="empty-state">暂无恢复记录</p>
      </div>
    </Panel>
  </div>
</template>
