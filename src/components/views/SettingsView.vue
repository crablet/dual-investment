<script setup lang="ts">
// 设置视图。
// 组合 Binance 凭据连接和本地文件日志面板；各面板保持独立状态与操作。
import BinanceConnectionPanel from '@/components/settings/BinanceConnectionPanel.vue'
import SettingsLogPanel from '@/components/settings/SettingsLogPanel.vue'
</script>

<template>
  <div class="settings-page">
    <BinanceConnectionPanel />
    <SettingsLogPanel />
  </div>
</template>

<style scoped>
.settings-page {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 12px;
  align-items: start;
}
</style>
