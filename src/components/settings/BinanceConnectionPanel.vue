<script setup lang="ts">
import { KeyRound } from '@lucide/vue'
import { NAlert, NButton, NIcon, NInput } from 'naive-ui'
import { computed, reactive } from 'vue'
import Panel from '@/components/common/Panel.vue'
import { useAppStore } from '@/stores/app'

const store = useAppStore()

const form = reactive({
  apiKey: '',
  apiSecret: '',
})

const canConnect = computed(() => form.apiKey.trim().length > 0 && form.apiSecret.length > 0)
const backendStatus = computed(() => store.settings?.connected ? '已连接' : '等待连接')
const backendStatusClass = computed(() => store.settings?.connected ? 'positive' : 'warning')
const credentialStatus = computed(() => store.settings?.savedCredentials ? '已保存' : '未保存')
const credentialStatusClass = computed(() => store.settings?.savedCredentials ? 'positive' : 'warning')
const reconnectAlertType = computed(() => store.autoReconnectPaused ? 'warning' : 'info')

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
</script>

<template>
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
          <NAlert v-if="store.autoReconnectMessage" :type="reconnectAlertType" class="settings-reconnect-alert">
            {{ store.autoReconnectMessage }}
          </NAlert>
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
</template>
