<script setup lang="ts">
import { computed } from 'vue'
import { AlertTriangle, ShieldCheck } from 'lucide-vue-next'
import { NButton, NIcon, NProgress, NTag } from 'naive-ui'
import Panel from '@/components/common/Panel.vue'
import { useFormat } from '@/composables/useFormat'
import { useAppStore } from '@/stores/app'
import type { RiskSeverity } from '@/types/domain'

const store = useAppStore()
const { money, percent } = useFormat()

const exposureRows = computed(() => {
  const rows = [
    { label: '卖出看涨义务仓', value: exposureByDirection('sellCall') },
    { label: '卖出看跌义务仓', value: exposureByDirection('sellPut') },
  ]
  const max = Math.max(...rows.map(row => row.value), 1)
  return rows.map(row => ({
    ...row,
    width: `${Math.max((row.value / max) * 100, row.value > 0 ? 10 : 0)}%`,
  }))
})

function exposureByDirection(direction: 'sellPut' | 'sellCall') {
  return store.orders
    .filter(order => order.direction === direction)
    .reduce((sum, order) => sum + Number(order.strike) * Number(order.quantity), 0)
}

function tagType(severity: RiskSeverity) {
  if (severity === 'high') return 'error'
  if (severity === 'medium') return 'warning'
  return 'info'
}
</script>

<template>
  <div class="view-grid">
    <Panel title="风险等级" class="span-3">
      <div class="risk-score">
        <NIcon :component="ShieldCheck" />
        <strong>{{ store.risk?.score ?? 0 }}</strong>
        <span>{{ store.risk?.level }}</span>
        <NProgress type="line" :percentage="store.risk?.score ?? 0" :show-indicator="false" status="warning" />
      </div>
    </Panel>

    <Panel title="保证金占用率" class="span-3">
      <div class="risk-metric"><strong>{{ percent(store.risk?.marginUsage ?? '0') }}</strong><span>保证金占用</span></div>
      <NProgress :percentage="Number(store.risk?.marginUsage ?? 0)" :show-indicator="false" />
    </Panel>

    <Panel title="被行权风险" class="span-3">
      <div class="risk-metric"><strong>{{ percent(store.risk?.exerciseRisk ?? '0') }}</strong><span>{{ store.risk?.level }}</span></div>
      <NProgress :percentage="Number(store.risk?.exerciseRisk ?? 0)" status="warning" :show-indicator="false" />
    </Panel>

    <Panel title="单日最大回撤" class="span-3">
      <div class="risk-metric"><strong>{{ percent(store.risk?.dailyDrawdown ?? '0') }}</strong><span>本地记录</span></div>
      <NProgress :percentage="Number(store.risk?.dailyDrawdown ?? 0) * 10" status="success" :show-indicator="false" />
    </Panel>

    <Panel title="风险暴露" class="span-6">
      <div class="bar-list exposure">
        <div v-for="row in exposureRows" :key="row.label">
          <span>{{ row.label }}</span>
          <strong :style="{ width: row.width }">{{ money(String(row.value)) }}</strong>
        </div>
      </div>
    </Panel>

    <Panel title="预警中心" class="span-6">
      <div class="alert-list">
        <p v-if="!store.risk?.alerts.length" class="empty-state">暂无风控预警</p>
        <article v-for="alert in store.risk?.alerts" :key="alert.id">
          <NTag :type="tagType(alert.severity)" size="small">{{ alert.severity === 'medium' ? '中等' : alert.severity === 'high' ? '高' : '低' }}</NTag>
          <div>
            <strong>{{ alert.title }}</strong>
            <span>{{ alert.message }}</span>
          </div>
          <NButton size="small" secondary>查看</NButton>
        </article>
      </div>
    </Panel>

    <Panel title="限制规则" class="span-12">
      <div class="rule-toggles">
        <article><span>单日最大执行次数</span><strong>{{ store.risk?.rules.maxDailyOrders }} 次/日</strong></article>
        <article><span>单标的最大占用</span><strong>{{ percent(store.risk?.rules.maxMarginUsage ?? '0') }}</strong></article>
        <article><span>最低目标年化</span><strong>{{ percent(store.autoRule?.minAnnualized ?? '0') }}</strong></article>
        <article><span>最大回撤阈值</span><strong>{{ percent(store.risk?.rules.maxDrawdown ?? '0') }}</strong></article>
        <article><span>连续亏损停止</span><strong>连续亏损 {{ store.risk?.rules.stopAfterConsecutiveLosses ?? 0 }} 次</strong></article>
        <article><span>下单确认</span><strong>逐笔确认</strong></article>
      </div>
    </Panel>

    <Panel title="近期风控事件" class="span-12">
      <div class="table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th>时间</th>
              <th>事件类型</th>
              <th>事件描述</th>
              <th>风险值</th>
              <th>状态</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="alert in store.risk?.alerts" :key="alert.id">
              <td>{{ alert.createdAt.slice(11, 19) }}</td>
              <td><NIcon :component="AlertTriangle" /> 风险预警</td>
              <td>{{ alert.message }}</td>
              <td>{{ alert.severity }}</td>
              <td><NTag :type="tagType(alert.severity)" size="small">预警中</NTag></td>
            </tr>
          </tbody>
        </table>
        <p v-if="!store.risk?.alerts.length" class="empty-state">暂无风控事件</p>
      </div>
    </Panel>
  </div>
</template>
