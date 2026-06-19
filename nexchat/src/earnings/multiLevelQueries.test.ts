import { describe, expect, it } from "vitest";
import { bpsToPercentLabel } from "@/earnings/multiLevelQueries";

describe("earnings/multiLevelQueries", () => {
  it("bpsToPercentLabel formats tier rate", () => {
    expect(bpsToPercentLabel(600)).toBe("6.00%");
    expect(bpsToPercentLabel(1200)).toBe("12.00%");
    expect(bpsToPercentLabel(null)).toBeNull();
  });
});
