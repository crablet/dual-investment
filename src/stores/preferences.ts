// 设备上的界面偏好独立于交易状态；语言切换不会重新挂载业务页面。
import type { AppLocale } from '@/i18n'
import { defineStore } from 'pinia'
import { computed, shallowRef } from 'vue'
import { i18n, loadLocale, resolveLocale, supportedLocales, t } from '@/i18n'

const storageKey = 'dual-investment.locale'
type LocalePreference = AppLocale | 'system'

function readPreference(): LocalePreference {
  try {
    const value = localStorage.getItem(storageKey)
    return supportedLocales.includes(value as AppLocale) ? value as AppLocale : 'system'
  }
  catch {
    return 'system'
  }
}

export const usePreferencesStore = defineStore('preferences', () => {
  const localePreference = shallowRef<LocalePreference>('en')
  const pendingLocalePreference = shallowRef<LocalePreference | null>(null)
  const localeLoadFailed = shallowRef(false)
  const locale = computed(() => i18n.global.locale.value as AppLocale)
  const isLocaleLoading = computed(() => pendingLocalePreference.value !== null)
  let localeRequest = 0

  function updateDocumentLocale() {
    document.documentElement.lang = locale.value
    document.title = t('ui.appTitle')
  }

  async function applyLocale(value: LocalePreference, persist: boolean) {
    if (value !== 'system' && !supportedLocales.includes(value))
      return

    const request = ++localeRequest
    const target = value === 'system' ? resolveLocale(navigator.languages) : value
    pendingLocalePreference.value = value
    localeLoadFailed.value = false
    try {
      await loadLocale(target)
      // 较早的词典可以进入缓存，但不能覆盖用户后选的语言。
      if (request !== localeRequest)
        return
      i18n.global.locale.value = target
      localePreference.value = value
      updateDocumentLocale()
      if (persist) {
        try {
          localStorage.setItem(storageKey, value)
        }
        catch {
          // 存储受限时仍允许本次会话切换语言。
        }
      }
    }
    catch {
      if (request === localeRequest)
        localeLoadFailed.value = true
    }
    finally {
      if (request === localeRequest)
        pendingLocalePreference.value = null
    }
  }

  async function initializeLocale() {
    updateDocumentLocale()
    await applyLocale(readPreference(), false)
  }

  function setLocale(value: LocalePreference) {
    return applyLocale(value, true)
  }

  function refreshSystemLocale() {
    if ((pendingLocalePreference.value ?? localePreference.value) === 'system')
      void setLocale('system')
  }

  return { localePreference, pendingLocalePreference, localeLoadFailed, locale, isLocaleLoading, initializeLocale, setLocale, refreshSystemLocale }
})
