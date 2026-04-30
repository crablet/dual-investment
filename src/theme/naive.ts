import type { GlobalThemeOverrides } from 'naive-ui'

const accentColor = '#f5b625'
const accentColorHover = '#ffcf55'
const accentColorPressed = '#c58e14'
const accentFocusShadow = '0 0 0 2px rgba(245, 182, 37, 0.24)'

// 全局 Naive UI 主题覆盖；业务组件只消费主题，不在根组件内堆叠样式常量。
export const themeOverrides: GlobalThemeOverrides = {
  common: {
    primaryColor: accentColor,
    primaryColorHover: accentColorHover,
    primaryColorPressed: accentColorPressed,
    primaryColorSuppl: accentColor,
    bodyColor: '#071017',
    cardColor: '#101922',
    modalColor: '#101922',
    borderColor: 'rgba(255,255,255,0.09)',
    textColorBase: '#f4f7fb',
    textColor1: '#f4f7fb',
    textColor2: '#c5ced9',
    textColor3: '#8d98a6',
  },
  Switch: {
    railColorActive: accentColor,
    loadingColor: accentColor,
    boxShadowFocus: accentFocusShadow,
  },
  Slider: {
    fillColor: accentColor,
    fillColorHover: accentColorHover,
    dotBorderActive: `2px solid ${accentColor}`,
    handleBoxShadowFocus: accentFocusShadow,
  },
  Checkbox: {
    colorChecked: accentColor,
    borderChecked: `1px solid ${accentColor}`,
    borderFocus: `1px solid ${accentColor}`,
    checkMarkColor: '#071017',
    boxShadowFocus: accentFocusShadow,
  },
  Progress: {
    fillColor: accentColor,
    fillColorInfo: accentColor,
    fillColorWarning: accentColorHover,
  },
}
