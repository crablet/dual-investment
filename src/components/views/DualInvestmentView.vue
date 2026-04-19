<script setup lang="ts">
import { ArrowDownCircle, ArrowUpCircle, PlusCircle, Zap } from 'lucide-vue-next'
import { NButton, NIcon, NInput, NInputNumber } from 'naive-ui'
import { computed, reactive, watch } from 'vue'
import CandidateTable from '@/components/common/CandidateTable.vue'
import MiniLineChart from '@/components/common/MiniLineChart.vue'
import Panel from '@/components/common/Panel.vue'
import { useFormat } from '@/composables/useFormat'
import { useAppStore } from '@/stores/app'
import type { StrategyCandidate, StrategyDirection } from '@/types/domain'

const store = useAppStore()
const { money, percent } = useFormat()

const form = reactive({
  direction: 'sellPut' as StrategyDirection,
  strike: 72000,
  quantity: 0.01,
  limitPrice: 0,
  expiry: store.candidates[0]?.expiry ?? new Date(Date.now() + 14 * 86400_000).toISOString().slice(0, 10),
  optionSymbol: '',
})

const matchingCandidate = computed(() =>
  store.candidates.find(candidate =>
    candidate.direction === form.direction
    && candidate.expiry === form.expiry
    && Number(candidate.strike) === form.strike,
  ),
)
const selectedCandidate = computed(() =>
  store.candidates.find(candidate => candidate.optionSymbol === form.optionSymbol)
  ?? matchingCandidate.value,
)
const canSubmit = computed(() =>
  Boolean(store.settings?.connected)
  && form.strike > 0
  && form.quantity >= 0.01
  && form.limitPrice > 0,
)

watch(
  () => [form.direction, form.expiry, form.strike] as const,
  () => {
    form.optionSymbol = matchingCandidate.value?.optionSymbol ?? ''
  },
  { immediate: true },
)

function applyCandidate(candidate: StrategyCandidate) {
  form.direction = candidate.direction
  form.strike = Number(candidate.strike)
  form.quantity = Number(candidate.quantity)
  form.limitPrice = Number(candidate.limitPrice)
  form.expiry = candidate.expiry
  form.optionSymbol = candidate.optionSymbol
}

function submit() {
  if (!window.confirm('确认发送 Binance 实盘期权订单？')) {
    return
  }
  void store.placeStrategy({
    candidateId: selectedCandidate.value?.id,
    symbol: 'BTCUSDT',
    optionSymbol: form.optionSymbol.trim() || selectedCandidate.value?.optionSymbol,
    direction: form.direction,
    expiry: form.expiry,
    strike: String(form.strike),
    quantity: form.quantity.toFixed(2),
    limitPrice: String(form.limitPrice),
    confirmLive: true,
  })
}
</script>

<template>
  <div class="view-grid">
    <Panel title="新建策略" subtitle="快速成交，后台同步到期状态并执行 ITM 现货补齐" class="span-5">
      <div class="strategy-form">
        <div class="segmented">
          <button :class="{ active: form.direction === 'sellPut' }" type="button" @click="form.direction = 'sellPut'">
            <NIcon :component="ArrowDownCircle" />
            卖 Put 抄底
          </button>
          <button :class="{ active: form.direction === 'sellCall' }" type="button" @click="form.direction = 'sellCall'">
            <NIcon :component="ArrowUpCircle" />
            卖 Call 止盈
          </button>
        </div>
        <label>
          执行价 USDT
          <NInputNumber v-model:value="form.strike" :min="1" :step="100" />
        </label>
        <label>
          下单数量 张
          <NInputNumber v-model:value="form.quantity" :min="0.01" :step="0.01" />
        </label>
        <label>
          权利金限价 USDT
          <NInputNumber v-model:value="form.limitPrice" :min="0.01" :step="0.01" />
        </label>
        <label>
          到期日
          <input v-model="form.expiry" class="native-input" type="date">
        </label>
        <label>
          Binance 期权合约
          <NInput v-model:value="form.optionSymbol" placeholder="BTC-260626-72000-P" />
        </label>
        <div class="button-row">
          <NButton type="primary" size="large" :disabled="!canSubmit" @click="submit">
            <template #icon><NIcon :component="Zap" /></template>
            发送实盘订单
          </NButton>
          <NButton secondary size="large" @click="store.activeView = 'auto'">
            <template #icon><NIcon :component="PlusCircle" /></template>
            加入自动规则
          </NButton>
        </div>
      </div>
    </Panel>

    <Panel title="策略收益示意" class="span-7">
      <div class="payoff-panel">
        <MiniLineChart v-if="store.pnl" :points="store.pnl.points.slice(0, 7)" />
        <div v-if="selectedCandidate" class="payoff-stats">
          <article>
            <span>最大收益</span>
            <strong>{{ money(selectedCandidate.premium) }}</strong>
          </article>
          <article>
            <span>单位限价</span>
            <strong>{{ money(selectedCandidate.limitPrice) }}</strong>
          </article>
          <article>
            <span>保本价</span>
            <strong>{{ money(selectedCandidate.breakeven) }}</strong>
          </article>
          <article>
            <span>年化收益</span>
            <strong class="positive">{{ percent(selectedCandidate.annualized) }}</strong>
          </article>
          <article>
            <span>被行权概率</span>
            <strong class="warning">{{ percent(selectedCandidate.exerciseProbability) }}</strong>
          </article>
        </div>
      </div>
    </Panel>

    <Panel title="策略摘要" class="span-12">
      <ul class="summary-list">
        <li>卖出看跌期权，若 BTC 低于执行价到期，则自动用预留 USDT 买入 BTC。</li>
        <li>卖出看涨期权，若 BTC 高于执行价到期，则自动卖出预留 BTC 完成止盈。</li>
        <li>快速成交使用单位限价 IOC，未成交、部分成交和拒绝状态都会写入本地订单。</li>
      </ul>
    </Panel>

    <Panel title="今日可选机会" class="span-12">
      <CandidateTable :candidates="store.candidates" @order="applyCandidate" />
    </Panel>
  </div>
</template>
