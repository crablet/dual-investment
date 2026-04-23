<script setup lang="ts">
// 设置视图。
// 负责 Binance 凭据连接、keyring 删除和审计导出；API Secret 提交后立即清空本地表单字段。
import { Download, KeyRound } from '@lucide/vue'
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
  // Secret 只传给 Rust keyring 写入流程，前端表单随后立即清空。
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
  // 导出内容来自后端脱敏审计日志，不包含 Secret 或原始 Binance 响应。
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
        <div class="settings-primary">
          <div class="settings-connection-status" aria-label="Binance 连接状态">
            <div>
              <span>连接</span>
              <strong :class="backendStatusClass">{{ backendStatus }}</strong>
            </div>
            <div>
              <span>凭据</span>
              <strong :class="store.settings?.savedCredentials ? 'positive' : 'warning'">
                {{ store.settings?.savedCredentials ? '已保存' : '未保存' }}
              </strong>
            </div>
          </div>

          <div class="settings-connect-form">
            <NInput v-model:value="form.apiKey" placeholder="API Key" />
            <NInput v-model:value="form.apiSecret" type="password" placeholder="API Secret" show-password-on="click" />
            <NButton type="primary" :disabled="!canConnect" :loading="store.isLoading('connect')" @click="connect">
              <template #icon><NIcon :component="KeyRound" /></template>
              连接 Binance
            </NButton>
            <NButton
              secondary
              :disabled="!store.settings?.savedCredentials"
              :loading="store.isLoading('connect')"
              @click="connectSaved"
            >
              <template #icon><NIcon :component="KeyRound" /></template>
              使用已保存凭据连接
            </NButton>
          </div>
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
        <NButton
          size="small"
          secondary
          :disabled="!store.settings?.savedCredentials"
          :loading="store.isLoading('credentials')"
          @click="forgetCredentials"
        >
          删除
        </NButton>
      </div>
    </Panel>
  </div>
</template>

<style scoped>
.settings-page {
  display: grid;
  grid-template-columns: minmax(0, 1.45fr) minmax(280px, 0.75fr);
  gap: 12px;
  align-items: start;
}

.settings-connection {
  display: grid;
  grid-template-columns: minmax(300px, 1fr) minmax(190px, 0.68fr);
  gap: 20px;
  align-items: start;
}

.settings-primary {
  display: grid;
  gap: 14px;
  min-width: 0;
}

.settings-connection-status {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--line);
}

.settings-connection-status div {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-width: 0;
  min-height: 32px;
}

.settings-connection-status span {
  min-width: 0;
  color: var(--muted);
}

.settings-connection-status strong {
  color: var(--text);
  font-weight: 700;
  white-space: nowrap;
}

.settings-connection-status strong.positive {
  color: var(--green);
}

.settings-connection-status strong.warning {
  color: var(--gold);
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

.settings-permissions li {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  min-width: 0;
  min-height: 38px;
  border-bottom: 1px solid var(--line);
}

.settings-permissions li:last-child {
  border-bottom: 0;
}

.settings-permissions span {
  min-width: 0;
  color: var(--muted);
}

.settings-permissions strong {
  margin: 0;
  color: var(--text);
  font-weight: 700;
  white-space: nowrap;
}

.settings-permissions strong.warning {
  color: var(--gold);
}

.settings-operation-row {
  display: grid;
  grid-template-columns: 24px minmax(0, 1fr) auto auto;
  gap: 12px;
  align-items: center;
  min-width: 0;
  min-height: 40px;
  border-bottom: 1px solid var(--line);
}

.settings-operation-row:last-child {
  border-bottom: 0;
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

@media (max-width: 1080px) {
  .settings-page,
  .settings-connection {
    grid-template-columns: 1fr;
  }

  .settings-permissions {
    padding-top: 16px;
    padding-left: 0;
    border-top: 1px solid var(--line);
    border-left: 0;
  }
}
</style>
