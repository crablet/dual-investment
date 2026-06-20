/** 只计算条形图的相对宽度，标签上的金额由快照提供。 */
export function relativeBarWidth(value: number, maxValue: number) {
  return maxValue > 0 ? `${Math.max(0, value) / maxValue * 100}%` : '0%'
}
