import { describe, expect, it } from "vitest";
import { formatMcr } from "lib/shipDesignTemplates";

describe("formatMcr", () => {
  it("writes a price the way the books do", () => {
    expect(formatMcr(274.25)).toBe("MCr 274.25");
    expect(formatMcr(160.187)).toBe("MCr 160.19");
    expect(formatMcr(1343.5)).toMatch(/^MCr 1.?343\.5$/);
  });

  it("says nothing for a design with no price", () => {
    expect(formatMcr(undefined)).toBeNull();
    expect(formatMcr(null)).toBeNull();
  });
});
