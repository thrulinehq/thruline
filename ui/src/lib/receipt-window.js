export const INITIAL_RECEIPT_COUNT = 4

const ROW_ID = /^row_[0-9a-f]{20}$/

export function receiptRegionId(rowId) {
  return ROW_ID.test(rowId) ? `wm-receipts-${rowId}` : null
}

export function receiptWindow(receipts, expanded = false) {
  const source = Array.isArray(receipts) ? receipts : []
  const total = source.length
  const isExpanded = total > INITIAL_RECEIPT_COUNT && expanded === true
  const visible = isExpanded ? source : source.slice(0, INITIAL_RECEIPT_COUNT)
  const items = visible.map((receipt, index) => ({
    receipt,
    position: index + 1,
    visibleLabel: `Receipt ${index + 1} of ${total}`,
    ariaLabel: `Open exact receipt ${index + 1} of ${total}`,
  }))

  return {
    total,
    items,
    expanded: isExpanded,
    hasDisclosure: total > INITIAL_RECEIPT_COUNT,
    controlLabel: total > INITIAL_RECEIPT_COUNT
      ? (isExpanded ? 'Show fewer receipts' : `Show all ${total} receipts`)
      : null,
  }
}
