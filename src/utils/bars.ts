// 将数值转换为条形图宽度。

/** 正值保留最小可见宽度，零值不画条形。 */
export function relativeBarWidth(value: number, maxValue: number) {
  const denominator = Math.max(maxValue, 1)
  return `${Math.max((value / denominator) * 100, value > 0 ? 10 : 0)}%`
}
