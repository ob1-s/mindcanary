import { describe, expect, it } from "vitest";

import type { AggregateBatch } from "@mindcanary/protocol";

import { MAX_PENDING_BATCHES, trimPendingBatchQueue } from "./queue";

function batch(sequence: number): AggregateBatch {
  return {
    batch_id: `batch-${sequence}`,
    source_instance_id: "source",
    sequence,
    period: {
      start: "2026-06-15T12:00:00Z",
      end: "2026-06-15T12:15:00Z",
      time_zone: "UTC",
    },
    metrics: [],
  };
}

describe("pending batch queue", () => {
  it("keeps the newest batches and counts dropped overflow", () => {
    const queue = {
      pendingBatches: Array.from({ length: MAX_PENDING_BATCHES + 4 }, (_, i) =>
        batch(i),
      ),
      droppedBatchCount: 0,
    };

    trimPendingBatchQueue(queue);

    expect(queue.pendingBatches).toHaveLength(MAX_PENDING_BATCHES);
    expect(queue.pendingBatches[0]?.batch_id).toBe("batch-4");
    expect(queue.pendingBatches.at(-1)?.batch_id).toBe("batch-99");
    expect(queue.droppedBatchCount).toBe(4);
    expect(JSON.stringify(queue)).not.toMatch(
      /https?:|url|title|history|page text|search term/i,
    );
  });

  it("preserves an existing dropped count", () => {
    const queue = {
      pendingBatches: [batch(1), batch(2), batch(3)],
      droppedBatchCount: 7,
    };

    trimPendingBatchQueue(queue, 2);

    expect(queue.pendingBatches.map((item) => item.batch_id)).toEqual([
      "batch-2",
      "batch-3",
    ]);
    expect(queue.droppedBatchCount).toBe(8);
  });
});
