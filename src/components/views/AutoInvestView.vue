<script setup lang="ts">
import { Save, Search } from 'lucide-vue-next'
import { NButton, NCheckbox, NIcon, NInputNumber, NSlider, NSwitch } from 'naive-ui'
import { computed, reactive, watchEffect } from 'vue'
import Panel from '@/components/common/Panel.vue'
import { useFormat } from '@/composables/useFormat'
import { useAppStore } from '@/stores/app'

const store = useAppStore()
const { money, percent } = useFormat()

interface RuleForm {
  enabled: boolean
  schedule: string
  quantity: number
  minAnnualized: number
  maxDeltaAbs: number
  maxIvRank: number
  minDaysToExpiry: number
  maxDaysToExpiry: number
  includeBtc: boolean
  excludeSameDirection: boolean
}

const form = reactive<RuleForm>({
  enabled: true,
  schedule: '每天 11:30',
  quantity: 0.01,
  minAnnualized: 15,
  maxDeltaAbs: 0.30,
  maxIvRank: 60,
  minDaysToExpiry: 7,
  maxDaysToExpiry: 45,
  includeBtc: true,
  excludeSameDirection: true,
})

watchEffect(() => {
  if (!store.autoRule) {
    return
  }
  Object.assign(form, {
    ...store.autoRule,
    quantity: Number(store.autoRule.quantity),
    minAnnualized: Number(store.autoRule.minAnnualized),
    maxDeltaAbs: Number(store.autoRule.maxDeltaAbs),
    maxIvRank: Number(store.autoRule.maxIvRank),
  })
})

const recommended = computed(() => store.candidates.slice(0, 3))

function save() {
  void store.saveRule({
    ...form,
    quantity: form.quantity.toFixed(2),
    minAnnualized: String(form.minAnnualized),
    maxDeltaAbs: form.maxDeltaAbs.toFixed(2),
    maxIvRank: String(form.maxIvRank),
  })
}

function toggle(value: boolean) {
  void store.toggleAuto({
    enabled: value,
    confirmLive: store.settings?.mode !== 'live' || window.confirm('确认开启实盘自动定投？'),
  })
}
</script>

<template>
  <div class="view-grid">
    <Panel title="1. 定投频率" class="span-7">
      <div class="form-grid">
        <label>
          扫描计划
          <input v-model="form.schedule" class="native-input">
        </label>
        <label>
          每次下单数量
          <NInputNumber v-model:value="form.quantity" :min="0.01" :step="0.01" />
        </label>
      </div>
    </Panel>

    <Panel title="策略预览 / 今日扫描结果" class="span-5 row-span-3">
      <div class="scan-list">
        <article v-for="candidate in recommended" :key="candidate.id" :class="candidate.recommendation">
          <header>
            <strong>{{ candidate.direction === 'sellPut' ? 'BTC 双币赢 抄底' : 'BTC 双币赢 止盈' }}</strong>
            <span>{{ candidate.recommendation === 'recommended' ? '推荐' : candidate.recommendation === 'watch' ? '观察' : '放弃' }}</span>
          </header>
          <p>执行价 <b>{{ money(candidate.strike) }}</b></p>
          <p>到期 <b>{{ candidate.expiry }}</b></p>
          <p>预计年化 <b :class="{ positive: candidate.recommendation !== 'reject', negative: candidate.recommendation === 'reject' }">{{ percent(candidate.annualized) }}</b></p>
          <em>{{ candidate.reason }}</em>
        </article>
      </div>
    </Panel>

    <Panel title="2. 自动筛选条件" class="span-7">
      <div class="rule-grid">
        <label>
          预计年化 ≥ {{ percent(form.minAnnualized) }}
          <NSlider v-model:value="form.minAnnualized" :min="5" :max="60" :step="1" />
        </label>
        <label>
          Delta 绝对值 ≤ {{ form.maxDeltaAbs }}
          <NSlider v-model:value="form.maxDeltaAbs" :min="0.05" :max="0.8" :step="0.01" />
        </label>
        <label>
          IV Rank ≤ {{ percent(form.maxIvRank, 0) }}
          <NSlider v-model:value="form.maxIvRank" :min="10" :max="100" :step="1" />
        </label>
        <div class="range-row">
          <span>到期天数</span>
          <NInputNumber v-model:value="form.minDaysToExpiry" :min="1" />
          <span>至</span>
          <NInputNumber v-model:value="form.maxDaysToExpiry" :min="form.minDaysToExpiry" />
        </div>
        <div class="checkbox-row">
          <NCheckbox v-model:checked="form.includeBtc">BTC</NCheckbox>
          <NCheckbox v-model:checked="form.excludeSameDirection">排除已持有相同方向策略</NCheckbox>
        </div>
      </div>
    </Panel>

    <Panel title="3. 执行保护" class="span-7">
      <div class="protection-grid">
        <p>单笔最大亏损限制 <strong>{{ money(store.risk?.rules.maxSingleOrderUsdt ?? '500') }}</strong></p>
        <p>单日最大执行次数 <strong>{{ store.risk?.rules.maxDailyOrders ?? '20' }} 次</strong></p>
        <p>连续亏损停止 <strong>{{ store.risk?.rules.stopAfterConsecutiveLosses ?? 3 }} 次</strong></p>
        <p>市场剧烈波动暂停 <strong>已启用</strong></p>
      </div>
      <template #action>
        <NSwitch :value="store.settings?.autoEnabled ?? false" @update:value="toggle" />
      </template>
    </Panel>

    <Panel title="4. 保存执行" class="span-12">
      <div class="button-row">
        <NButton type="primary" size="large" @click="save">
          <template #icon><NIcon :component="Save" /></template>
          保存并启动自动定投
        </NButton>
        <NButton secondary size="large" @click="store.scan">
          <template #icon><NIcon :component="Search" /></template>
          立即扫描
        </NButton>
      </div>
    </Panel>
  </div>
</template>
