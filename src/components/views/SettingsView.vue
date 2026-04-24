<script setup lang="ts">
// 设置视图。
// 负责 Binance 凭据连接、keyring 删除和本地文件日志查看；API Secret 提交后立即清空本地表单字段。
import { FileText, FolderOpen, KeyRound, RefreshCw, Trash2 } from '@lucide/vue'
import { NAlert, NButton, NIcon, NInput, NTag } from 'naive-ui'
import { computed, reactive } from 'vue'
import Panel from '@/components/common/Panel.vue'
import { useLogSnapshot } from '@/composables/useLogSnapshot'
import { useAppStore } from '@/stores/app'
import type { LogFileInfo } from '@/types/domain'
import { localDateTimeLabel } from '@/utils/date'
import { logLevelTone } from '@/utils/labels'

const store = useAppStore()
const logs = useLogSnapshot()

const form = reactive({
  apiKey: '',
  apiSecret: '',
})

const canConnect = computed(() => form.apiKey.trim().length > 0 && form.apiSecret.length > 0)
const backendStatus = computed(() => store.settings?.connected ? '已连接' : '等待连接')
const backendStatusClass = computed(() => store.settings?.connected ? 'positive' : 'warning')
const credentialStatus = computed(() => store.settings?.savedCredentials ? '已保存' : '未保存')
const credentialStatusClass = computed(() => store.settings?.savedCredentials ? 'positive' : 'warning')
const latestLogLabel = computed(() => {
  const latest = logs.recentEntries.value[0]
  return latest ? localDateTimeLabel(latest.timestamp) : '暂无记录'
})

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

async function clearLogs() {
  if (!window.confirm('确认删除全部本地日志文件？订单、恢复记录和 keyring 凭据不会被删除。')) {
    return
  }
  await logs.clear()
}

function fileSizeLabel(bytes: number) {
  if (bytes < 1024) {
    return `${bytes} B`
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`
  }
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}

function modifiedLabel(file: LogFileInfo) {
  return file.modifiedAt ? localDateTimeLabel(file.modifiedAt) : '暂无修改时间'
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
              <div class="settings-status-actions">
                <strong :class="credentialStatusClass">{{ credentialStatus }}</strong>
                <NButton
                  v-if="store.settings?.savedCredentials"
                  size="tiny"
                  secondary
                  :loading="store.isLoading('credentials')"
                  @click="forgetCredentials"
                >
                  删除
                </NButton>
              </div>
            </div>
          </div>

          <div class="settings-connect-form">
            <NInput v-model:value="form.apiKey" placeholder="API Key" />
            <NInput v-model:value="form.apiSecret" type="password" placeholder="API Secret" show-password-on="click" />
            <div class="settings-connect-actions">
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

    <Panel title="日志" class="settings-logs">
      <template #action>
        <NButton size="small" secondary :loading="logs.pendingAction.value === 'refresh'" @click="logs.refresh">
          <template #icon><NIcon :component="RefreshCw" /></template>
          刷新
        </NButton>
      </template>

      <NAlert v-if="logs.error.value" type="error" class="settings-log-error">
        {{ logs.error.value }}
      </NAlert>

      <div class="settings-log-summary">
        <div>
          <span>今日日志</span>
          <strong>{{ logs.currentFileName.value }}</strong>
        </div>
        <div>
          <span>日志文件</span>
          <strong>{{ logs.fileCountLabel.value }}</strong>
        </div>
        <div>
          <span>最近记录</span>
          <strong>{{ latestLogLabel }}</strong>
        </div>
      </div>

      <div class="settings-log-path">
        <span>目录</span>
        <strong>{{ logs.snapshot.value?.directory ?? '等待读取' }}</strong>
      </div>

      <div class="settings-log-actions">
        <NButton secondary :loading="logs.pendingAction.value === 'openFile'" @click="logs.openCurrentFile">
          <template #icon><NIcon :component="FileText" /></template>
          打开今日日志
        </NButton>
        <NButton secondary :loading="logs.pendingAction.value === 'openFolder'" @click="logs.openFolder">
          <template #icon><NIcon :component="FolderOpen" /></template>
          打开日志文件夹
        </NButton>
        <NButton secondary type="error" :loading="logs.pendingAction.value === 'clear'" :disabled="logs.files.value.length === 0" @click="clearLogs">
          <template #icon><NIcon :component="Trash2" /></template>
          删除日志
        </NButton>
      </div>

      <section class="settings-log-section">
        <h3>最近日志</h3>
        <div v-if="logs.recentEntries.value.length === 0" class="settings-empty-row">
          暂无日志记录
        </div>
        <div v-for="entry in logs.recentEntries.value" :key="entry.id" class="settings-log-entry">
          <time>{{ localDateTimeLabel(entry.timestamp) }}</time>
          <NTag :type="logLevelTone(entry.level)" size="small" class="settings-log-level">
            {{ entry.level }}
          </NTag>
          <span class="settings-log-event">{{ entry.event }}</span>
          <p>{{ entry.message }}</p>
        </div>
      </section>

      <section class="settings-log-section">
        <h3>日志文件</h3>
        <div v-if="logs.files.value.length === 0" class="settings-empty-row">
          暂无日志文件
        </div>
        <div v-for="file in logs.files.value" :key="file.path" class="settings-log-file">
          <strong>{{ file.date }}</strong>
          <span>{{ file.entryCount }} 条</span>
          <span>{{ fileSizeLabel(file.sizeBytes) }}</span>
          <time>{{ modifiedLabel(file) }}</time>
        </div>
      </section>
    </Panel>
  </div>
</template>

<style scoped>
.settings-page {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
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

.settings-connection-status > div {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-width: 0;
  min-height: 32px;
}

.settings-connection-status .settings-status-actions {
  display: inline-flex;
  align-items: center;
  justify-content: flex-end;
  gap: 10px;
  min-width: 0;
}

.settings-connection-status span,
.settings-log-summary span,
.settings-log-path span {
  min-width: 0;
  color: var(--muted);
}

.settings-connection-status strong,
.settings-log-summary strong,
.settings-log-path strong {
  color: var(--text);
  font-weight: 700;
}

.settings-connection-status strong {
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

.settings-connect-actions {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.settings-connect-actions :deep(.n-button) {
  width: 100%;
}

.settings-permissions {
  display: grid;
  gap: 12px;
  min-width: 0;
  padding-left: 24px;
  border-left: 1px solid var(--line);
}

.settings-permissions h3,
.settings-log-section h3 {
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

.settings-logs {
  min-width: 0;
}

.settings-log-error {
  margin-bottom: 12px;
}

.settings-log-summary {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--line);
}

.settings-log-summary div,
.settings-log-path {
  display: grid;
  gap: 4px;
  min-width: 0;
}

.settings-log-summary strong,
.settings-log-path strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.settings-log-path {
  padding: 12px 0;
  border-bottom: 1px solid var(--line);
}

.settings-log-actions {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px;
  padding: 12px 0;
  border-bottom: 1px solid var(--line);
}

.settings-log-actions :deep(.n-button) {
  width: 100%;
}

.settings-log-section {
  display: grid;
  gap: 8px;
  padding-top: 14px;
}

.settings-empty-row {
  min-height: 42px;
  padding: 10px 0;
  color: var(--muted);
}

.settings-log-entry {
  display: grid;
  grid-template-columns: 150px 64px minmax(220px, 0.55fr) minmax(0, 1.45fr);
  gap: 12px;
  align-items: start;
  min-width: 0;
  padding: 9px 0;
  border-bottom: 1px solid var(--line);
}

.settings-log-entry:last-child,
.settings-log-file:last-child {
  border-bottom: 0;
}

.settings-log-entry time,
.settings-log-file time,
.settings-log-file span {
  color: var(--muted);
  font-variant-numeric: tabular-nums;
}

.settings-log-level {
  justify-self: center;
  min-width: 52px;
}

.settings-log-level :deep(.n-tag__content) {
  display: inline-flex;
  justify-content: center;
  width: 100%;
  text-align: center;
  text-transform: lowercase;
}

.settings-log-event {
  overflow: hidden;
  color: var(--muted);
  text-overflow: ellipsis;
  white-space: nowrap;
}

.settings-log-entry p {
  min-width: 0;
  margin: 0;
  overflow-wrap: anywhere;
}

.settings-log-file {
  display: grid;
  grid-template-columns: 88px 64px 82px minmax(0, 1fr);
  gap: 10px;
  align-items: center;
  min-width: 0;
  min-height: 36px;
  border-bottom: 1px solid var(--line);
}

.settings-log-file strong {
  font-variant-numeric: tabular-nums;
}

@media (max-width: 1180px) {
  .settings-connection {
    grid-template-columns: 1fr;
  }

  .settings-permissions {
    padding-top: 16px;
    padding-left: 0;
    border-top: 1px solid var(--line);
    border-left: 0;
  }

  .settings-log-entry {
    grid-template-columns: 142px 54px minmax(0, 1fr);
  }

  .settings-log-entry p {
    grid-column: 1 / -1;
  }
}
</style>
