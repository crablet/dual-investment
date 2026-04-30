import type { StrategyOrder } from '@/types/domain'

/** 合并后端返回的更新订单，并按更新时间倒序展示。 */
export function mergeOrdersByUpdatedAt(next: StrategyOrder[], current: StrategyOrder[]) {
  const byId = new Map(current.map(order => [order.id, order]))
  for (const order of next) {
    byId.set(order.id, order)
  }
  return [...byId.values()].sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))
}
