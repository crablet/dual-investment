<script setup lang="ts">
import { computed, shallowRef, watchEffect } from 'vue'
import { NButton, NTag } from 'naive-ui'
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
</script>

<template>
  <div class="view-grid">
    <Panel title="订单统计" class="span-12">
      <div class="summary-cards">
        <article><span>总订单数</span><strong>{{ store.orders.length }}</strong><em>较昨日 +12.28%</em></article>
        <article><span>进行中订单</span><strong>{{ store.runningOrders.length }}</strong><em>占比 28.13%</em></article>
        <article><span>今日到期</span><strong>8</strong><em>预计权利金 ¥2,186.72</em></article>
        <article><span>累计收取权利金</span><strong>¥ 48,862.37</strong><em>较昨日 +1.28%</em></article>
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
              <td>{{ order.quantity }}</td>
              <td>{{ money(order.premium) }}</td>
              <td><NTag :type="statusTone(order.status)" size="small">{{ statusText(order.status) }}</NTag></td>
            </tr>
          </tbody>
        </table>
      </div>
    </Panel>

    <Panel title="订单详情" class="span-4">
      <div v-if="selected" class="detail-list">
        <h3>{{ selected.id }}</h3>
        <NTag :type="statusTone(selected.status)">{{ statusText(selected.status) }}</NTag>
        <p><span>方向</span><strong>{{ directionText(selected.direction) }}</strong></p>
        <p><span>标的</span><strong>{{ selected.symbol }}</strong></p>
        <p><span>期权合约</span><strong>{{ selected.optionSymbol }}</strong></p>
        <p><span>执行价</span><strong>{{ money(selected.strike) }}</strong></p>
        <p><span>数量</span><strong>{{ selected.quantity }} 张</strong></p>
        <p><span>权利金</span><strong>{{ money(selected.premium) }}</strong></p>
        <p><span>保本价</span><strong>{{ money(selected.breakeven) }}</strong></p>
        <p><span>到期补齐</span><strong>{{ selected.settlementAction }}</strong></p>
        <div class="button-row compact">
          <NButton secondary @click="store.activeView = 'portfolio'">查看持仓</NButton>
          <NButton secondary @click="store.activeView = 'dual'">复制策略</NButton>
        </div>
      </div>
    </Panel>
  </div>
</template>
