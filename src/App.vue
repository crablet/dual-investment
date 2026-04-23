<script setup lang="ts">
// 根组件负责全局 UI provider、异步视图路由和后台静默同步。
// 业务状态全部放在 Pinia store，视图组件只消费当前快照和触发显式操作。
import { darkTheme, dateZhCN, NConfigProvider, NDialogProvider, NMessageProvider, zhCN } from 'naive-ui'
import type { GlobalThemeOverrides } from 'naive-ui'
import { computed, defineAsyncComponent, onBeforeUnmount, onMounted } from 'vue'
import AppShell from '@/components/layout/AppShell.vue'
import { useAppStore } from '@/stores/app'

const store = useAppStore()

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
let syncTimer: number | undefined

// Naive UI 主题覆盖集中放在根组件，确保所有动态加载视图共享同一套暗色交易终端风格。
const accentColor = '#f5b625'
const accentColorHover = '#ffcf55'
const accentColorPressed = '#c58e14'
const accentFocusShadow = '0 0 0 2px rgba(245, 182, 37, 0.24)'

const themeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: accentColor,
    primaryColorHover: accentColorHover,
    primaryColorPressed: accentColorPressed,
    primaryColorSuppl: accentColor,
    bodyColor: '#071017',
    cardColor: '#101922',
    modalColor: '#101922',
    borderColor: 'rgba(255,255,255,0.09)',
    textColorBase: '#f4f7fb',
    textColor1: '#f4f7fb',
    textColor2: '#c5ced9',
    textColor3: '#8d98a6',
  },
  Switch: {
    railColorActive: accentColor,
    loadingColor: accentColor,
    boxShadowFocus: accentFocusShadow,
  },
  Slider: {
    fillColor: accentColor,
    fillColorHover: accentColorHover,
    dotBorderActive: `2px solid ${accentColor}`,
    handleBoxShadowFocus: accentFocusShadow,
  },
  Checkbox: {
    colorChecked: accentColor,
    borderChecked: `1px solid ${accentColor}`,
    borderFocus: `1px solid ${accentColor}`,
    checkMarkColor: '#071017',
    boxShadowFocus: accentFocusShadow,
  },
  Progress: {
    fillColor: accentColor,
    fillColorInfo: accentColor,
    fillColorWarning: accentColorHover,
  },
}

onMounted(() => {
  void store.bootstrap()
  // 后台同步使用 silent 请求，不清空前台错误提示，也不会自动开启自动策略。
  syncTimer = window.setInterval(() => {
    if (store.settings?.connected) {
      void store.syncLiveState({ silent: true })
    }
  }, 60_000)
})

onBeforeUnmount(() => {
  if (syncTimer !== undefined) {
    window.clearInterval(syncTimer)
  }
})
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
