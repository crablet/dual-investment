<script setup lang="ts">
import { Download, KeyRound } from 'lucide-vue-next'
import { NButton, NIcon, NInput } from 'naive-ui'
import { computed, reactive } from 'vue'
import Panel from '@/components/common/Panel.vue'
import { api } from '@/services/api'
import { useAppStore } from '@/stores/app'

const store = useAppStore()
const form = reactive({
  apiKey: '',
  apiSecret: '',
})

const canConnect = computed(() => form.apiKey.trim().length > 0 && form.apiSecret.length > 0)
const backendStatus = computed(() => store.settings?.connected ? '已连接' : '等待连接')
const backendStatusClass = computed(() => store.settings?.connected ? 'positive' : 'warning')
const auditCountLabel = computed(() => `${store.auditRecent.length} 条`)

async function connect() {
  if (!canConnect.value) {
    return
  }

  const request = {
    apiKey: form.apiKey.trim(),
    apiSecret: form.apiSecret,
  }
  form.apiSecret = ''
  await store.connect(request)
}

async function connectSaved() {
  await store.connectSaved()
}

async function forgetCredentials() {
  if (!window.confirm('确认从系统 keyring 删除 Binance API 凭据？这会断开当前连接。')) {
    return
  }
  await store.forgetCredentials()
}

async function exportLogs() {
  const payload = await api.exportAuditLogs()
  const blob = new Blob([payload], { type: 'application/json;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  link.href = url
  link.download = `dual-investment-audit-${new Date().toISOString().slice(0, 10)}.json`
  link.click()
  URL.revokeObjectURL(url)
  window.alert('审计日志已导出。')
}
</script>

<template>
  <div class="settings-page">
    <Panel title="Binance 连接" class="settings-main">
      <div class="settings-connection">
        <div class="settings-connect-form">
          <NInput v-model:value="form.apiKey" placeholder="API Key" />
          <NInput v-model:value="form.apiSecret" type="password" placeholder="API Secret" show-password-on="click" />
          <NButton type="primary" :disabled="!canConnect" :loading="store.loading" @click="connect">
            <template #icon><NIcon :component="KeyRound" /></template>
            连接 Binance
          </NButton>
          <NButton
            secondary
            :disabled="!store.settings?.savedCredentials"
            :loading="store.loading"
            @click="connectSaved"
          >
            <template #icon><NIcon :component="KeyRound" /></template>
            使用已保存凭据连接
          </NButton>
        </div>

        <section class="settings-permissions" aria-label="API 权限">
          <h3>权限</h3>
          <ul>
            <li><span>读取账户</span><strong>需要</strong></li>
            <li><span>读取行情</span><strong>需要</strong></li>
            <li><span>现货交易</span><strong>需要</strong></li>
            <li><span>期权交易</span><strong>需要</strong></li>
            <li><span>提现</span><strong class="warning">禁止</strong></li>
          </ul>
        </section>
      </div>
    </Panel>

    <Panel title="运行状态" class="settings-side">
      <dl class="settings-status-list">
        <div>
          <dt>运行环境</dt>
          <dd>Tauri 客户端</dd>
        </div>
        <div>
          <dt>后台</dt>
          <dd :class="backendStatusClass">{{ backendStatus }}</dd>
        </div>
        <div>
          <dt>存储</dt>
          <dd>SQLite + 系统 keyring</dd>
        </div>
        <div>
          <dt>凭据</dt>
          <dd :class="store.settings?.savedCredentials ? 'positive' : 'warning'">
            {{ store.settings?.savedCredentials ? '已保存到 keyring' : '未保存' }}
          </dd>
        </div>
        <div>
          <dt>版本</dt>
          <dd>v0.1.0</dd>
        </div>
      </dl>
    </Panel>

    <Panel title="安全与日志" class="settings-ops">
      <div class="settings-operation-row">
        <NIcon :component="Download" />
        <span>审计日志</span>
        <strong>{{ auditCountLabel }}</strong>
        <NButton size="small" secondary @click="exportLogs">导出</NButton>
      </div>
      <div class="settings-operation-row">
        <NIcon :component="KeyRound" />
        <span>系统 keyring 凭据</span>
        <strong>{{ store.settings?.savedCredentials ? '已保存' : '无' }}</strong>
        <NButton size="small" secondary :disabled="!store.settings?.savedCredentials" @click="forgetCredentials">删除</NButton>
      </div>
    </Panel>
  </div>
</template>

<style scoped>
.settings-page {
  display: grid;
  grid-template-columns: minmax(0, 1.55fr) minmax(300px, 0.85fr);
  gap: 12px;
  align-items: start;
}

.settings-ops {
  grid-column: 1 / -1;
}

.settings-connection {
  display: grid;
  grid-template-columns: minmax(320px, 1fr) minmax(220px, 0.72fr);
  gap: 24px;
  align-items: start;
}

.settings-connect-form {
  display: grid;
  gap: 12px;
}

.settings-connect-form :deep(.n-button) {
  width: 100%;
}

.settings-permissions {
  display: grid;
  gap: 12px;
  min-width: 0;
  padding-left: 24px;
  border-left: 1px solid var(--line);
}

.settings-permissions h3 {
  margin: 0;
  color: var(--muted);
  font-size: 13px;
  font-weight: 600;
}

.settings-permissions ul {
  display: grid;
  gap: 0;
  margin: 0;
  padding: 0;
  list-style: none;
}

.settings-permissions li,
.settings-status-list div {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  min-width: 0;
  min-height: 38px;
  border-bottom: 1px solid var(--line);
}

.settings-permissions li:last-child,
.settings-status-list div:last-child {
  border-bottom: 0;
}

.settings-permissions span,
.settings-status-list dt {
  min-width: 0;
  color: var(--muted);
}

.settings-permissions strong,
.settings-status-list dd {
  margin: 0;
  color: var(--text);
  font-weight: 700;
  white-space: nowrap;
}

.settings-permissions strong.warning,
.settings-status-list dd.warning {
  color: var(--gold);
}

.settings-status-list dd.positive {
  color: var(--green);
}

.settings-status-list {
  display: grid;
  gap: 0;
  margin: 0;
}

.settings-operation-row {
  display: grid;
  grid-template-columns: 24px minmax(0, 1fr) auto auto;
  gap: 12px;
  align-items: center;
  min-height: 40px;
}

.settings-operation-row .n-icon {
  color: var(--muted);
  font-size: 20px;
}

.settings-operation-row span {
  min-width: 0;
}

.settings-operation-row strong {
  color: var(--muted);
  font-weight: 600;
  white-space: nowrap;
}

@media (max-width: 1200px) {
  .settings-page,
  .settings-connection {
    grid-template-columns: 1fr;
  }

  .settings-ops {
    grid-column: auto;
  }

  .settings-permissions {
    padding-top: 16px;
    padding-left: 0;
    border-top: 1px solid var(--line);
    border-left: 0;
  }
}
</style>
