// 自动复投规则表单状态。
//
// 后端仍是自动策略开启、二次确认和实盘执行门禁的最终来源；这里只维护页面表单、
// 用户确认和提交前的 Decimal 字符串契约。

import { reactive, shallowRef, watchEffect } from 'vue'
import { useAppStore } from '@/stores/app'
import { decimalStringToDisplayNumber } from '@/utils/decimal'

export interface AutoRuleForm {
  /** UI 开关状态，保存规则时不代表后端自动策略已开启。 */
  enabled: boolean
  schedule: string
  quantity: number
  minAnnualized: number
  maxDeltaAbs: number
  maxIvRank: number
  minDaysToExpiry: number
  maxDaysToExpiry: number
  includeBtc: boolean
  excludeSameDirection: boolean
}

/** 组合自动复投规则表单、保存动作和真实执行前的用户确认。 */
export function useAutoRuleForm() {
  const store = useAppStore()
  const advancedAutoOpen = shallowRef(false)

  const ruleForm = reactive<AutoRuleForm>({
    enabled: false,
    schedule: '每天 11:30',
    quantity: 0.01,
    minAnnualized: 15,
    maxDeltaAbs: 0.30,
    maxIvRank: 60,
    minDaysToExpiry: 7,
    maxDaysToExpiry: 45,
    includeBtc: true,
    excludeSameDirection: true,
  })

  watchEffect(() => {
    if (!store.autoRule) {
      return
    }
    // 后端 DTO 用 Decimal 字符串承接金融值；本地表单转 number 只是为了 Naive UI 控件交互。
    Object.assign(ruleForm, {
      ...store.autoRule,
      quantity: decimalStringToDisplayNumber(store.autoRule.quantity),
      minAnnualized: decimalStringToDisplayNumber(store.autoRule.minAnnualized),
      maxDeltaAbs: decimalStringToDisplayNumber(store.autoRule.maxDeltaAbs),
      maxIvRank: decimalStringToDisplayNumber(store.autoRule.maxIvRank),
    })
  })

  function saveAutoRule() {
    // 表单用 number 便于 slider/input 交互，提交前转回 Decimal 字符串契约。
    void store.saveRule({
      ...ruleForm,
      quantity: ruleForm.quantity.toFixed(2),
      minAnnualized: String(ruleForm.minAnnualized),
      maxDeltaAbs: ruleForm.maxDeltaAbs.toFixed(2),
      maxIvRank: String(ruleForm.maxIvRank),
    })
  }

  function toggleAuto(value: boolean) {
    // 自动复投开启后符合条件的自动单不会逐笔确认，因此开关本身必须二次确认。
    if (value && !window.confirm('确认开启自动定投？后续符合条件的自动单不会逐笔确认。')) {
      return
    }
    void store.toggleAuto({
      enabled: value,
      confirmLive: true,
    })
  }

  function runAutoNow() {
    // 立即执行可能直接下真实订单，必须显式确认。
    if (!window.confirm('确认立即执行一次自动扫描？若存在推荐候选，将直接发送 Binance 订单。')) {
      return
    }
    void store.runAutoOnce()
  }

  return {
    advancedAutoOpen,
    ruleForm,
    saveAutoRule,
    toggleAuto,
    runAutoNow,
  }
}
