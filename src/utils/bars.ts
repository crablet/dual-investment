// 轻量条形图展示工具。
//
// 页面仍负责计算业务值，这里只统一把一组非负展示值转换为现有 .bar-list 使用的宽度。

/** 计算条形图相对宽度；正值至少 10%，零值保持 0%，和原页面内联公式一致。 */
export function relativeBarWidth(value: number, maxValue: number) {
  const denominator = Math.max(maxValue, 1)
  return `${Math.max((value / denominator) * 100, value > 0 ? 10 : 0)}%`
}
