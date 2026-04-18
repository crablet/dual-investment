<script setup lang="ts">
import { AlertTriangle, ShieldCheck } from 'lucide-vue-next'
import { NButton, NIcon, NProgress, NSwitch, NTag } from 'naive-ui'
import Panel from '@/components/common/Panel.vue'
import { useFormat } from '@/composables/useFormat'
import { useAppStore } from '@/stores/app'
import type { RiskSeverity } from '@/types/domain'

const store = useAppStore()
const { money, percent } = useFormat()

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
      <div class="risk-metric"><strong>{{ percent(store.risk?.marginUsage ?? '0') }}</strong><span>¥ 8,942.31 / ¥ 21,128.44</span></div>
      <NProgress :percentage="Number(store.risk?.marginUsage ?? 0)" :show-indicator="false" />
    </Panel>

    <Panel title="被行权风险" class="span-3">
      <div class="risk-metric"><strong>{{ percent(store.risk?.exerciseRisk ?? '0') }}</strong><span>中等风险</span></div>
      <NProgress :percentage="Number(store.risk?.exerciseRisk ?? 0)" status="warning" :show-indicator="false" />
    </Panel>

    <Panel title="单日最大回撤" class="span-3">
      <div class="risk-metric"><strong>{{ percent(store.risk?.dailyDrawdown ?? '0') }}</strong><span>累计 -1,248.36 USDT</span></div>
      <NProgress :percentage="Number(store.risk?.dailyDrawdown ?? 0) * 10" status="success" :show-indicator="false" />
    </Panel>

    <Panel title="风险暴露" class="span-6">
      <div class="bar-list exposure">
        <div><span>卖出看涨（双币）</span><strong style="width: 48%">¥ 32,154.27</strong></div>
        <div><span>卖出看跌（双币）</span><strong style="width: 32%">¥ 21,843.60</strong></div>
        <div><span>自动定投策略</span><strong style="width: 16%">¥ 6,845.21</strong></div>
        <div><span>现货持仓</span><strong style="width: 10%">¥ 4,128.16</strong></div>
      </div>
    </Panel>

    <Panel title="预警中心" class="span-6">
      <div class="alert-list">
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
        <article><span>单日最大执行次数</span><strong>{{ store.risk?.rules.maxDailyOrders }} 次/日</strong><NSwitch :value="true" /></article>
        <article><span>单标的最大占用</span><strong>50%</strong><NSwitch :value="true" /></article>
        <article><span>最低目标年化</span><strong>15.00%</strong><NSwitch :value="true" /></article>
        <article><span>极端波动暂停</span><strong>24H 波动率 ≥ 80%</strong><NSwitch :value="true" /></article>
        <article><span>连续亏损停止</span><strong>连续亏损 {{ store.risk?.rules.stopAfterConsecutiveLosses }} 次</strong><NSwitch :value="true" /></article>
        <article><span>手动确认阈值</span><strong>单笔占用 ≥ 20%</strong><NSwitch :value="false" /></article>
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
              <td class="positive">{{ alert.severity === 'low' ? '2.48%' : '42.31%' }}</td>
              <td><NTag :type="tagType(alert.severity)" size="small">预警中</NTag></td>
            </tr>
          </tbody>
        </table>
      </div>
    </Panel>
  </div>
</template>
