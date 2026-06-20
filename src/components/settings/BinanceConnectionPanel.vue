<!-- 连接 Binance 并显示凭据状态。提交时清空表单中的 Secret，凭据由 Rust 存入系统 keyring。 -->
<script setup lang="ts">
import { KeyRound } from '@lucide/vue'

import { NAlert, NButton, NIcon, NInput } from 'naive-ui'
import { computed, reactive } from 'vue'
import Panel from '@/components/common/Panel.vue'
import { t } from '@/i18n'
import { useAppStore } from '@/stores/app'

const store = useAppStore()

const form = reactive({
  apiKey: '',
  apiSecret: '',
})

const canConnect = computed(() => form.apiKey.trim().length > 0 && form.apiSecret.length > 0)
const backendStatus = computed(() => store.settings?.connected ? t('ui.connected') : t('ui.awaitingConnection'))
const backendStatusClass = computed(() => store.settings?.connected ? 'positive' : 'warning')
const credentialStatus = computed(() => store.settings?.savedCredentials ? t('ui.saved') : t('ui.notSaved'))
const credentialStatusClass = computed(() => store.settings?.savedCredentials ? 'positive' : 'warning')
const reconnectAlertType = computed(() => store.autoReconnectPaused ? 'warning' : 'info')

async function connect() {
  if (!canConnect.value || store.isLoading('connect')) {
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
  if (!window.confirm(t('ui.deleteBinanceApiCredentialsFromTheSystem'))) {
    return
  }
  await store.forgetCredentials()
}
</script>

<template>
  <Panel :title="t('ui.binanceConnection')" class="settings-main">
    <div class="settings-connection">
      <div class="settings-primary">
        <div class="settings-connection-status" :aria-label="t('ui.binanceConnectionStatus')">
          <div>
            <span> {{ t('ui.connection') }} </span>
            <strong :class="backendStatusClass">{{ backendStatus }}</strong>
          </div>
          <div>
            <span> {{ t('ui.credentials') }} </span>
            <div class="settings-status-actions">
              <strong :class="credentialStatusClass">{{ credentialStatus }}</strong>
              <NButton
                v-if="store.settings?.savedCredentials"
                size="tiny"
                secondary
                :loading="store.isLoading('credentials')"
                @click="forgetCredentials"
              >
                {{ t('ui.delete') }}
              </NButton>
            </div>
          </div>
        </div>

        <form class="settings-connect-form" method="post" @submit.prevent="connect">
          <label for="binance-api-key">{{ t('ui.apiKey') }}</label>
          <NInput v-model:value="form.apiKey" :input-props="{ 'id': 'binance-api-key', 'name': 'apiKey', 'aria-label': t('ui.apiKey'), 'required': true, 'autocomplete': 'off', 'spellcheck': false, 'autocapitalize': 'none', 'autocorrect': 'off' }" />
          <label for="binance-api-secret">{{ t('ui.apiSecret') }}</label>
          <NInput v-model:value="form.apiSecret" type="password" :input-props="{ 'id': 'binance-api-secret', 'name': 'apiSecret', 'aria-label': t('ui.apiSecret'), 'required': true, 'autocomplete': 'off', 'spellcheck': false, 'autocapitalize': 'none', 'autocorrect': 'off' }" show-password-on="click" />
          <div class="settings-connect-actions">
            <NButton type="primary" attr-type="submit" :disabled="!canConnect" :loading="store.isLoading('connect')">
              <template #icon><NIcon aria-hidden="true" :component="KeyRound" /></template> {{ t('ui.connectBinance') }}
            </NButton>
            <NButton
              secondary
              attr-type="button"
              :disabled="!store.settings?.savedCredentials"
              :loading="store.isLoading('connect')"
              @click="connectSaved"
            >
              <template #icon><NIcon aria-hidden="true" :component="KeyRound" /></template> {{ t('ui.connectWithSavedCredentials') }}
            </NButton>
          </div>
          <NAlert v-if="store.autoReconnectMessage" :type="reconnectAlertType" class="settings-reconnect-alert">
            {{ store.autoReconnectMessage }}
          </NAlert>
        </form>
      </div>

      <section class="settings-permissions" :aria-label="t('ui.apiPermissions')">
        <h3> {{ t('ui.permission') }} </h3>
        <ul>
          <li><span> {{ t('ui.readAccount') }} </span><strong> {{ t('ui.required') }} </strong></li>
          <li><span> {{ t('ui.readMarketData') }} </span><strong> {{ t('ui.required') }} </strong></li>
          <li><span> {{ t('ui.spotTrading') }} </span><strong> {{ t('ui.required') }} </strong></li>
          <li><span> {{ t('ui.optionsTrading') }} </span><strong> {{ t('ui.required') }} </strong></li>
          <li><span> {{ t('ui.withdrawals') }} </span><strong class="warning"> {{ t('ui.prohibited') }} </strong></li>
        </ul>
      </section>
    </div>
  </Panel>
</template>

<style scoped>
.settings-main {
  container-type: inline-size;
}

.settings-connection {
  display: grid;
  grid-template-columns: minmax(300px, 1.4fr) minmax(190px, 0.8fr);
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

.settings-connect-form label {
  margin-bottom: -7px;
  color: var(--muted);
  font-size: 13px;
}

.settings-connect-actions {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.settings-connect-actions :deep(.n-button) {
  width: 100%;
  min-height: 34px;
  height: auto;
  padding-block: 6px;
}

.settings-connect-actions :deep(.n-button__content) {
  white-space: normal;
  line-height: 1.5;
}

.settings-reconnect-alert {
  min-width: 0;
}

.settings-permissions {
  display: grid;
  gap: 12px;
  min-width: 0;
  padding: 16px 20px;
  border-radius: 10px;
  background: var(--inset);
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

@container (max-width: 760px) {
  .settings-connection-status,
  .settings-connect-actions {
    grid-template-columns: 1fr;
  }
}

@container (max-width: 540px) {
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
