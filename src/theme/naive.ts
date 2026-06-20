import type { GlobalThemeOverrides } from 'naive-ui'

const gold = '#f3c542'
const goldHover = '#ffda68'
const goldPressed = '#d8ac31'
const ink = '#211c0c'

export const themeOverrides: GlobalThemeOverrides = {
  common: {
    fontFamily: 'inherit',
    fontSize: '14px',
    borderRadius: '7px',
    primaryColor: gold,
    primaryColorHover: goldHover,
    primaryColorPressed: goldPressed,
    primaryColorSuppl: gold,
    bodyColor: '#111315',
    cardColor: '#1a1d20',
    modalColor: '#1e2125',
    popoverColor: '#292d31',
    inputColor: '#141618',
    borderColor: '#35393e',
    dividerColor: '#303438',
    textColorBase: '#f3f4f5',
    textColor1: '#f3f4f5',
    textColor2: '#d3d5d8',
    textColor3: '#a3a8ae',
    placeholderColor: '#91979d',
    successColor: '#61d7aa',
    warningColor: gold,
    errorColor: '#ff9293',
  },
  Button: {
    textColorPrimary: ink,
    textColorHoverPrimary: ink,
    textColorPressedPrimary: ink,
    textColorFocusPrimary: ink,
    fontWeightStrong: '650',
  },
  Checkbox: {
    colorChecked: gold,
    borderChecked: `1px solid ${gold}`,
    borderFocus: `1px solid ${gold}`,
    checkMarkColor: ink,
    boxShadowFocus: '0 0 0 2px rgba(243, 197, 66, 0.24)',
  },
}
