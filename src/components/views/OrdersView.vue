<script setup lang="ts">
import type { StrategyOrder } from '@/types/domain'
// 策略订单视图。
// 展示真实订单、同步/结算操作和恢复记录；恢复动作会回到 Rust 重新查询 Binance。
import { RefreshCw, ScanLine } from '@lucide/vue'
import { NButton, NIcon, NTag } from 'naive-ui'
import { computed, shallowRef, watchEffect } from 'vue'
import Panel from '@/components/common/Panel.vue'
import { useFormat } from '@/composables/useFormat'
import { useAppStore } from '@/stores/app'
import { localDateKey, localDateTimeLabel } from '@/utils/date'

const store = useAppStore()
const { directionText, directionTone, money, statusText, statusTone } = useFormat()
const selectedId = shallowRef<string>()

interface OrderDetailRow {
  label: string
  value: string
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
const attemptRows = computed(() => store.orderAttempts.slice(0, 5))
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

    <Panel title="恢复记录" class="span-12">
      <div class="table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th>时间</th>
              <th>客户端订单号</th>
              <th>合约</th>
              <th>交易所订单号</th>
              <th>状态</th>
              <th>说明</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="attempt in attemptRows" :key="attempt.id">
              <td>{{ attempt.updatedAt.slice(0, 19).replace('T', ' ') }}</td>
              <td>{{ attempt.clientOrderId }}</td>
              <td>{{ attempt.optionSymbol }}</td>
              <td>{{ attempt.exchangeOrderId ?? '--' }}</td>
              <td>{{ attempt.status }}</td>
              <td>{{ attempt.message }}</td>
              <td>
                <NButton
                  size="small"
                  secondary
                  :disabled="attempt.status === 'persisted' || attempt.status === 'spot_persisted'"
                  @click="store.recoverAttempt(attempt.id)"
                >
                  恢复
                </NButton>
              </td>
            </tr>
          </tbody>
        </table>
        <p v-if="attemptRows.length === 0" class="empty-state">暂无需要恢复的订单尝试记录</p>
      </div>
    </Panel>
  </div>
</template>
