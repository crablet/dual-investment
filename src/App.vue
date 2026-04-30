<!--
根组件负责全局 UI provider、异步视图路由和后台静默同步。

业务状态全部放在 Pinia store，视图组件只消费当前快照和触发显式操作。
-->
<script setup lang="ts">
import { darkTheme, dateZhCN, NConfigProvider, NDialogProvider, NMessageProvider, zhCN } from 'naive-ui'
import { computed, defineAsyncComponent } from 'vue'
import AppShell from '@/components/layout/AppShell.vue'
import { useBackgroundSync } from '@/composables/useBackgroundSync'
import { useAppStore } from '@/stores/app'
import { themeOverrides } from '@/theme/naive'

const store = useAppStore()
useBackgroundSync()

const OverviewView = defineAsyncComponent(() => import('@/components/views/OverviewView.vue'))
const DualInvestmentView = defineAsyncComponent(() => import('@/components/views/DualInvestmentView.vue'))

const views = {
  overview: OverviewView,
  dual: DualInvestmentView,
  orders: defineAsyncComponent(() => import('@/components/views/OrdersView.vue')),
  portfolio: defineAsyncComponent(() => import('@/components/views/PortfolioView.vue')),
  analytics: defineAsyncComponent(() => import('@/components/views/AnalyticsView.vue')),
  risk: defineAsyncComponent(() => import('@/components/views/RiskView.vue')),
  settings: defineAsyncComponent(() => import('@/components/views/SettingsView.vue')),
}

const currentView = computed(() => views[store.activeView] ?? OverviewView)
</script>

<template>
  <NConfigProvider :theme="darkTheme" :theme-overrides="themeOverrides" :locale="zhCN" :date-locale="dateZhCN">
    <NDialogProvider>
      <NMessageProvider>
        <AppShell>
          <component :is="currentView" />
        </AppShell>
      </NMessageProvider>
    </NDialogProvider>
  </NConfigProvider>
</template>
