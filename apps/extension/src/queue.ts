import type { AggregateBatch } from "@mindcanary/protocol";

export const MAX_PENDING_BATCHES = 96;

export interface PendingBatchQueue {
  pendingBatches: AggregateBatch[];
  droppedBatchCount: number;
}

export function trimPendingBatchQueue(
  queue: PendingBatchQueue,
  limit = MAX_PENDING_BATCHES,
): void {
  const overflow = queue.pendingBatches.length - limit;
  if (overflow <= 0) {
    return;
  }

  queue.pendingBatches = queue.pendingBatches.slice(overflow);
  queue.droppedBatchCount += overflow;
}
