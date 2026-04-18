<script setup lang="ts">
import { Activity, FlaskConical, RefreshCw, Settings } from 'lucide-vue-next'
import { NAlert, NButton, NIcon, NSpin, NSwitch } from 'naive-ui'
import SidebarNav from '@/components/layout/SidebarNav.vue'
import { useAppStore } from '@/stores/app'

const store = useAppStore()

function toggleAuto(value: boolean) {
  void store.toggleAuto({
    enabled: value,
    confirmLive: store.settings?.mode !== 'live' || window.confirm('确认开启实盘自动策略？后续符合条件的自动单不会逐笔确认。'),
  })
}
</script>

<template>
  <div class="app-shell">
    <SidebarNav />
    <main class="workspace">
      <header class="topbar">
        <div class="brand-row">
          <div class="brand-mark">
            <NIcon :component="Activity" />
          </div>
          <div>
            <h1 class="brand-title">模拟双币赢</h1>
            <p class="brand-subtitle">小额现货 + 期权策略执行器</p>
          </div>
        </div>
        <div class="topbar-actions">
          <div class="status-pill success">
            <span />
            后台运行中
          </div>
          <div class="status-pill warning">
            <NIcon :component="FlaskConical" />
            {{ store.settings?.mode === 'live' ? '实盘模式' : '模拟模式' }}
          </div>
          <NSwitch
            :value="store.settings?.autoEnabled ?? false"
            :round="false"
            @update:value="toggleAuto"
          />
          <NButton quaternary circle :loading="store.loading" @click="store.refresh">
            <template #icon>
              <NIcon :component="RefreshCw" />
            </template>
          </NButton>
          <NButton quaternary circle @click="store.activeView = 'settings'">
            <template #icon>
              <NIcon :component="Settings" />
            </template>
          </NButton>
        </div>
      </header>

      <NAlert v-if="store.error" type="error" closable class="shell-alert">
        {{ store.error }}
      </NAlert>

      <NSpin :show="store.loading && !store.snapshot">
        <section class="view-surface">
          <slot />
        </section>
      </NSpin>
    </main>
  </div>
</template>
