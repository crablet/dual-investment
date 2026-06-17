<!-- 语言在连接交易所之前即可设置，选项使用各语言的原生名称。 -->
<script setup lang="ts">
import type { AppLocale } from '@/i18n'
import { computed } from 'vue'
import Panel from '@/components/common/Panel.vue'
import { t } from '@/i18n'
import { usePreferencesStore } from '@/stores/preferences'

const preferences = usePreferencesStore()
const languageNames: Record<AppLocale, string> = {
  'zh-CN': '简体中文',
  'en': 'English',
  'zh-TW': '繁體中文',
  'pt-BR': 'Português (Brasil)',
  'es': 'Español',
  'vi': 'Tiếng Việt',
  'tr': 'Türkçe',
  'id': 'Bahasa Indonesia',
  'ja': '日本語',
  'ko': '한국어',
  'th': 'ไทย',
  'eo': 'Esperanto',
  'fr': 'Français',
}
const options = computed(() => [
  { label: t('ui.system'), value: 'system' as const },
  ...Object.entries(languageNames)
    .map(([value, label]) => ({ value: value as AppLocale, label }))
    // 固定排序规则，切换界面语言时选项位置保持稳定。
    .sort((left, right) => left.label.localeCompare(right.label, 'en', { sensitivity: 'base' })),
])

function changeLanguage(event: Event) {
  if (event.target instanceof HTMLSelectElement)
    void preferences.setLocale(event.target.value as AppLocale | 'system')
}
</script>

<template>
  <Panel :title="t('ui.general')">
    <div class="language-setting">
      <label id="language-label" for="language-select">{{ t('ui.language') }}</label>
      <select
        id="language-select"
        :value="preferences.pendingLocalePreference ?? preferences.localePreference"
        :aria-busy="preferences.isLocaleLoading"
        :aria-describedby="preferences.localeLoadFailed ? 'language-error' : undefined"
        aria-labelledby="language-label"
        @change="changeLanguage"
      >
        <option v-for="option in options" :key="option.value" :value="option.value" :lang="option.value === 'system' ? preferences.locale : option.value">
          {{ option.label }}
        </option>
      </select>
      <p v-if="preferences.isLocaleLoading" role="status">
        {{ t('ui.languageLoading') }}
      </p>
      <p v-if="preferences.localeLoadFailed" id="language-error" class="language-error" role="alert">
        {{ t('ui.languageLoadFailed') }}
      </p>
    </div>
  </Panel>
</template>

<style scoped>
.language-setting {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 12px;
}
.language-setting label {
  font-weight: 600;
}
.language-setting p {
  margin: 0;
  color: var(--muted);
}
.language-setting .language-error {
  color: var(--gold);
}

.language-setting select {
  width: 100%;
  min-width: 0;
  min-height: 36px;
  padding: 6px 12px;
  border: 1px solid var(--line);
  border-radius: 4px;
  background: var(--surface);
  color: var(--text);
  font: inherit;
  cursor: pointer;
}

.language-setting select:focus-visible {
  outline: 2px solid var(--gold);
  outline-offset: 2px;
}
</style>
