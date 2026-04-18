<script setup lang="ts">
import { Download, KeyRound, RefreshCw, Trash2 } from 'lucide-vue-next'
import { NButton, NIcon, NInput, NSwitch } from 'naive-ui'
import { reactive } from 'vue'
import Panel from '@/components/common/Panel.vue'
import { api } from '@/services/api'
import { useAppStore } from '@/stores/app'

const store = useAppStore()
const form = reactive({
  apiKey: '',
  apiSecret: '',
  live: false,
})

function connect() {
  void store.connect({ ...form })
}

async function exportLogs() {
  const payload = await api.exportAuditLogs()
  console.info(payload)
  window.alert('审计日志已输出到开发者控制台。Tauri 打包版会返回 JSON 内容用于保存。')
}
</script>

<template>
  <div class="view-grid">
    <Panel title="账户与 API" class="span-8">
      <div class="settings-grid">
        <p><span>API 连接状态</span><strong :class="store.settings?.connected ? 'positive' : 'warning'">{{ store.settings?.connected ? '已连接' : '未连接' }}</strong></p>
        <p><span>连接账户</span><strong>Binance（现货 & 期权）</strong></p>
        <p><span>API Key</span><strong>{{ store.settings?.apiKeyMasked ?? '未配置' }}</strong></p>
        <p><span>权限要求</span><strong>读取账户、读取行情、现货交易、期权交易；禁止提现</strong></p>
      </div>
      <div class="connect-form">
        <NInput v-model:value="form.apiKey" placeholder="API Key" />
        <NInput v-model:value="form.apiSecret" type="password" placeholder="API Secret" show-password-on="click" />
        <label class="inline-control">
          实盘模式
          <NSwitch v-model:value="form.live" />
        </label>
        <NButton type="primary" @click="connect">
          <template #icon><NIcon :component="KeyRound" /></template>
          重新连接
        </NButton>
      </div>
    </Panel>

    <Panel title="当前环境" class="span-4">
      <div class="detail-list">
        <p><span>运行模式</span><strong>{{ store.settings?.mode === 'live' ? '实盘模式' : '模拟模式' }}</strong></p>
        <p><span>后台状态</span><strong class="positive">后台运行中</strong></p>
        <p><span>数据存储</span><strong>本地 SQLite</strong></p>
        <p><span>版本号</span><strong>v0.1.0</strong></p>
      </div>
    </Panel>

    <Panel title="通知设置" class="span-4">
      <div class="setting-list">
        <article><span>系统托盘通知</span><NSwitch :value="true" /></article>
        <article><span>提示音</span><NSwitch :value="true" /></article>
        <article><span>Telegram 通知</span><NSwitch :value="true" /></article>
        <article><span>邮件通知</span><NSwitch :value="false" /></article>
      </div>
    </Panel>

    <Panel title="应用偏好" class="span-4">
      <div class="setting-list">
        <article><span>开机启动</span><NSwitch :value="true" /></article>
        <article><span>最小化到托盘</span><NSwitch :value="true" /></article>
        <article><span>快速成交</span><NSwitch :value="store.settings?.quickExecution" /></article>
        <article><span>自动更新</span><NSwitch :value="true" /></article>
      </div>
    </Panel>

    <Panel title="安全与日志" class="span-4">
      <div class="security-list">
        <div class="security-action">
          <NIcon :component="Download" />
          <span>导出日志</span>
          <NButton class="security-action-button" size="small" secondary @click="exportLogs">导出</NButton>
        </div>
        <div class="security-action">
          <NIcon :component="Trash2" />
          <span>清除缓存</span>
          <NButton class="security-action-button" size="small" secondary>清除</NButton>
        </div>
        <div class="security-action">
          <NIcon :component="RefreshCw" />
          <span>检查更新</span>
          <NButton class="security-action-button" size="small" secondary>检查</NButton>
        </div>
      </div>
    </Panel>
  </div>
</template>
