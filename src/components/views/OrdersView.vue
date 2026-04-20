<script setup lang="ts">
import { RefreshCw, ScanLine } from 'lucide-vue-next'
import { computed, shallowRef, watchEffect } from 'vue'
import { NButton, NIcon, NTag } from 'naive-ui'
import Panel from '@/components/common/Panel.vue'
import { useFormat } from '@/composables/useFormat'
import { useAppStore } from '@/stores/app'

const store = useAppStore()
const { directionText, directionTone, money, statusText, statusTone } = useFormat()
const selectedId = shallowRef<string>()

watchEffect(() => {
  if (!selectedId.value && store.orders[0]) {
    selectedId.value = store.orders[0].id
  }
})

const selected = computed(() => store.orders.find(order => order.id === selectedId.value) ?? store.orders[0])
const totalPremium = computed(() =>
  store.orders.reduce((sum, order) => sum + Number(order.premium), 0),
)
const todayExpiryCount = computed(() => {
  const today = new Date().toISOString().slice(0, 10)
  return store.orders.filter(order => order.expiry === today).length
})
const attemptRows = computed(() => store.orderAttempts.slice(0, 5))
</script>

<template>
  <div class="view-grid">
    <Panel title="订单统计" class="span-12">
      <template #action>
        <div class="button-row compact">
          <NButton size="small" secondary :loading="store.loading" @click="store.syncLiveState()">
            <template #icon><NIcon :component="RefreshCw" /></template>
            同步订单
          </NButton>
          <NButton size="small" secondary :loading="store.loading" @click="store.runSettlementScan()">
            <template #icon><NIcon :component="ScanLine" /></template>
            结算扫描
          </NButton>
        </div>
      </template>
      <div class="summary-cards">
        <article><span>总订单数</span><strong>{{ store.orders.length }}</strong><em>本地订单</em></article>
        <article><span>进行中订单</span><strong>{{ store.runningOrders.length }}</strong><em>当前未结算</em></article>
        <article><span>今日到期</span><strong>{{ todayExpiryCount }}</strong><em>按本地订单统计</em></article>
        <article><span>累计权利金</span><strong>{{ money(String(totalPremium)) }}</strong><em>已记录订单</em></article>
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
              <td>{{ order.expiry }}</td>
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
      <div v-if="selected" class="detail-list">
        <h3>{{ selected.id }}</h3>
        <NTag :type="statusTone(selected.status)">{{ statusText(selected.status) }}</NTag>
        <p><span>方向</span><strong>{{ directionText(selected.direction) }}</strong></p>
        <p><span>标的</span><strong>{{ selected.symbol }}</strong></p>
        <p><span>期权合约</span><strong>{{ selected.optionSymbol }}</strong></p>
        <p><span>交易所状态</span><strong>{{ selected.exchangeStatus }}</strong></p>
        <p><span>客户端订单号</span><strong>{{ selected.clientOrderId }}</strong></p>
        <p><span>执行价</span><strong>{{ money(selected.strike) }}</strong></p>
        <p><span>成交数量</span><strong>{{ selected.filledQuantity }} / {{ selected.quantity }} 张</strong></p>
        <p><span>成交均价</span><strong>{{ money(selected.avgPrice) }}</strong></p>
        <p><span>手续费</span><strong>{{ money(selected.fee) }}</strong></p>
        <p><span>权利金</span><strong>{{ money(selected.premium) }}</strong></p>
        <p><span>保本价</span><strong>{{ money(selected.breakeven) }}</strong></p>
        <p><span>结算参考价</span><strong>{{ money(selected.settlementPrice) }}</strong></p>
        <p><span>现货订单号</span><strong>{{ selected.spotOrderId ?? '--' }}</strong></p>
        <p><span>现货状态</span><strong>{{ selected.spotOrderStatus || '--' }}</strong></p>
        <p><span>现货成交数量</span><strong>{{ selected.spotFilledQuantity }} BTC</strong></p>
        <p><span>现货成交均价</span><strong>{{ money(selected.spotAvgPrice) }}</strong></p>
        <p><span>现货手续费</span><strong>{{ selected.spotFee }}</strong></p>
        <p><span>到期补齐</span><strong>{{ selected.settlementAction }}</strong></p>
        <p><span>补齐结果</span><strong>{{ selected.rebalanceMessage || '--' }}</strong></p>
        <div class="button-row compact">
          <NButton secondary @click="store.activeView = 'portfolio'">查看持仓</NButton>
          <NButton secondary @click="store.activeView = 'dual'">复制策略</NButton>
        </div>
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
