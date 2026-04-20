<script setup lang="ts">
import {
  BarChart3,
  Bitcoin,
  ClipboardList,
  Home,
  Settings,
  ShieldCheck,
  Wallet,
} from '@lucide/vue'
import { NIcon } from 'naive-ui'
import { useAppStore } from '@/stores/app'

const store = useAppStore()

const navItems = [
  { id: 'overview', label: '总览', icon: Home },
  { id: 'dual', label: '双币赢', icon: Bitcoin },
  { id: 'orders', label: '策略订单', icon: ClipboardList },
  { id: 'portfolio', label: '持仓与现货', icon: Wallet },
  { id: 'analytics', label: '收益分析', icon: BarChart3 },
  { id: 'risk', label: '风险控制', icon: ShieldCheck },
  { id: 'settings', label: '设置', icon: Settings },
]
</script>

<template>
  <aside class="sidebar">
    <nav class="nav-list">
      <button
        v-for="item in navItems"
        :key="item.id"
        class="nav-item"
        :class="{ active: store.activeView === item.id }"
        type="button"
        @click="store.activeView = item.id"
      >
        <NIcon :component="item.icon" />
        <span>{{ item.label }}</span>
      </button>
    </nav>
  </aside>
</template>
