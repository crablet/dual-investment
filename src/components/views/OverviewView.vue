<script setup lang="ts">
import { computed } from 'vue'
import CandidateTable from '@/components/common/CandidateTable.vue'
import DonutChart from '@/components/common/DonutChart.vue'
import MetricCard from '@/components/common/MetricCard.vue'
import MiniLineChart from '@/components/common/MiniLineChart.vue'
import Panel from '@/components/common/Panel.vue'
import { useFormat } from '@/composables/useFormat'
import { useAppStore } from '@/stores/app'
import type { StrategyCandidate } from '@/types/domain'

const store = useAppStore()
const { money } = useFormat()

const activeStrategies = computed(() => store.orders.slice(0, 4))
const pnlCenter = computed(() => Number(store.pnl?.cumulative ?? 0).toLocaleString('zh-CN'))

function placeCandidate(candidate: StrategyCandidate) {
  void store.placeStrategy({
    candidateId: candidate.id,
    symbol: candidate.symbol,
    optionSymbol: candidate.optionSymbol,
    direction: candidate.direction,
    expiry: candidate.expiry,
    strike: candidate.strike,
    quantity: candidate.quantity,
    live: store.settings?.mode === 'live',
    confirmLive: store.settings?.mode !== 'live' || window.confirm('确认发送主网期权订单？'),
  })
}
</script>

<template>
  <div class="view-grid">
    <MetricCard v-for="metric in store.metrics" :key="metric.id" :metric="metric" />

    <Panel title="账户概览" class="span-3">
      <div class="account-list">
        <div v-for="balance in store.balances" :key="balance.asset" class="account-row">
          <span>{{ balance.asset }}</span>
          <strong>{{ balance.free }}</strong>
          <em>{{ money(balance.usdtValue) }}</em>
        </div>
      </div>
    </Panel>

    <Panel title="收益趋势" class="span-5">
      <MiniLineChart v-if="store.pnl" :points="store.pnl.points" />
    </Panel>

    <Panel title="收益构成" class="span-4">
      <DonutChart v-if="store.pnl" :items="store.pnl.breakdown" :center="pnlCenter" />
    </Panel>

    <Panel title="待处理事项" class="span-6">
      <div class="task-list">
        <div class="task-row">
          <span class="task-index">1</span>
          有 2 个双币订单即将到期
          <button type="button" @click="store.activeView = 'orders'">查看</button>
        </div>
        <div class="task-row">
          <span class="task-index warning">2</span>
          1 个策略订单缺少保证金补充
          <button type="button" @click="store.activeView = 'risk'">查看</button>
        </div>
        <div class="task-row">
          <span class="task-index blue">3</span>
          自动定投发现 3 个候选机会
          <button type="button" @click="store.activeView = 'auto'">查看</button>
        </div>
      </div>
    </Panel>

    <Panel title="自动定投状态" class="span-6">
      <div class="auto-state">
        <div>
          <span>下次扫描时间</span>
          <strong>01:23:45</strong>
          <em>预计 11:30 自动扫描市场</em>
        </div>
        <div class="switch-summary">
          <p>自动扫描 <strong>{{ store.autoRule?.enabled ? '已启用' : '已暂停' }}</strong></p>
          <p>快速成交 <strong>{{ store.settings?.quickExecution ? '已启用' : '未启用' }}</strong></p>
          <p>到期补齐 <strong>自动执行</strong></p>
        </div>
      </div>
    </Panel>

    <Panel title="快捷机会" class="span-12">
      <CandidateTable :candidates="store.candidates.slice(0, 3)" @order="placeCandidate" />
    </Panel>

    <Panel title="活跃策略" class="span-12">
      <div class="order-strip">
        <article v-for="order in activeStrategies" :key="order.id">
          <strong>{{ order.id }}</strong>
          <span>{{ order.optionSymbol }}</span>
          <em>{{ order.settlementAction }}</em>
        </article>
      </div>
    </Panel>
  </div>
</template>
