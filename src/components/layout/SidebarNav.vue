<!-- 导航统一调用 store.openView，重复点击也会刷新当前页面。 -->
<script setup lang="ts">
import type { Component } from 'vue'

import type { AppViewId } from '@/stores/app'
import {
  BarChart3,
  Bitcoin,
  ClipboardList,
  Home,
  Settings,
  Wallet,
} from '@lucide/vue'
import { NIcon } from 'naive-ui'
import { t } from '@/i18n'
import { useAppStore } from '@/stores/app'

const store = useAppStore()

const navItems = [
  { id: 'overview', get label() { return t('ui.overview') }, icon: Home },
  { id: 'dual', get label() { return t('ui.dualInvestment') }, icon: Bitcoin },
  { id: 'orders', get label() { return t('ui.strategyOrders') }, icon: ClipboardList },
  { id: 'portfolio', get label() { return t('ui.positionsSpot') }, icon: Wallet },
  { id: 'analytics', get label() { return t('ui.returns') }, icon: BarChart3 },
  { id: 'settings', get label() { return t('ui.settings') }, icon: Settings },
] as const satisfies ReadonlyArray<{ id: AppViewId, label: string, icon: Component }>
</script>

<template>
  <aside class="sidebar">
    <nav class="nav-list">
      <button
        v-for="item in navItems"
        :key="item.id"
        class="nav-item"
        :class="{ active: store.activeView === item.id }"
        :aria-current="store.activeView === item.id ? 'page' : undefined"
        type="button"
        @click="store.openView(item.id)"
      >
        <NIcon aria-hidden="true" :component="item.icon" />
        <span>{{ item.label }}</span>
      </button>
    </nav>
  </aside>
</template>
