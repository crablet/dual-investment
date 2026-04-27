<!--
风险控制视图。

展示 Rust 风控快照和义务仓暴露概览；下单拦截仍由 Rust 在实盘下单前执行。
-->
<script setup lang="ts">
import type { RiskSeverity } from '@/types/domain'
import { Activity, AlertTriangle, Gauge, ShieldCheck } from '@lucide/vue'
import { NButton, NIcon, NProgress, NTag } from 'naive-ui'
import { computed } from 'vue'
import Panel from '@/components/common/Panel.vue'
import { useFormat } from '@/composables/useFormat'
import { useAppStore } from '@/stores/app'

const store = useAppStore()
const { money, percent } = useFormat()

const riskLevelLabel = computed(() => store.risk?.level ?? '未同步')

const exposureRows = computed(() => {
  // 暴露条形图用于扫描相对大小，风控拦截仍以后端 RiskSnapshot 和下单前校验为准。
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

/**
 * 计算方向维度的名义暴露，用于 UI 条形图。
 *
 * Number 只用于展示比例；不要把这里的结果作为保证金或真实风险计算输入。
 */
function exposureByDirection(direction: 'sellPut' | 'sellCall') {
  return store.orders
    .filter(order => order.direction === direction)
    .reduce((sum, order) => sum + Number(order.strike) * Number(order.quantity), 0)
}

/** 映射 Naive UI 标签类型。 */
function tagType(severity: RiskSeverity) {
  if (severity === 'high')
    return 'error'
  if (severity === 'medium')
    return 'warning'
  return 'info'
}
</script>

<template>
  <div class="view-grid">
    <Panel title="风险等级" class="span-4">
      <div class="risk-score">
        <NIcon :component="ShieldCheck" />
        <strong>{{ store.risk?.score ?? 0 }}</strong>
        <span>{{ riskLevelLabel }}</span>
        <NProgress type="line" :percentage="store.risk?.score ?? 0" :show-indicator="false" status="warning" />
      </div>
    </Panel>

    <Panel title="保证金占用率" class="span-4">
      <div class="risk-score">
        <NIcon :component="Gauge" />
        <strong>{{ percent(store.risk?.marginUsage ?? '0') }}</strong>
        <span>{{ riskLevelLabel }}</span>
        <NProgress :percentage="Number(store.risk?.marginUsage ?? 0)" status="warning" :show-indicator="false" />
      </div>
    </Panel>

    <Panel title="被行权风险" class="span-4">
      <div class="risk-score">
        <NIcon :component="Activity" />
        <strong>{{ percent(store.risk?.exerciseRisk ?? '0') }}</strong>
        <span>{{ riskLevelLabel }}</span>
        <NProgress :percentage="Number(store.risk?.exerciseRisk ?? 0)" status="warning" :show-indicator="false" />
      </div>
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
          <NTag :type="tagType(alert.severity)" size="small" class="alert-severity-tag">
            {{ alert.severity === 'medium' ? '中等' : alert.severity === 'high' ? '高' : '低' }}
          </NTag>
          <div>
            <strong>{{ alert.title }}</strong>
            <span>{{ alert.message }}</span>
          </div>
          <NButton size="small" secondary>查看</NButton>
        </article>
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
