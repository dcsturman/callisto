import {describe, expect, test} from "vitest";
import {describeCourse} from "lib/courseMode";

describe("describeCourse", () => {
  test("an absent mode is a rendezvous, which is what older servers meant", () => {
    expect(describeCourse(undefined, "Tai'ao", false).headline).toBe("Intercept");
  });

  test("each rung says what it promises and names the target", () => {
    expect(describeCourse("Pursuit", "Tai'ao", false).detail).toContain(
      "Tai'ao cannot be matched",
    );
    expect(describeCourse("Shadow", "Tai'ao", false).detail).toContain(
      "pulling away faster than you can close",
    );
    expect(describeCourse("Intercept", "Tai'ao", false).detail).toContain(
      "matching its velocity",
    );
  });

  test("a blip is told its acceleration was not used", () => {
    const withBlip = describeCourse("Intercept", "Tai'ao", true).detail;
    expect(withBlip).toContain("No sensor contact on Tai'ao");
    expect(withBlip).toContain("holds its current velocity");
    expect(describeCourse("Intercept", "Tai'ao", false).detail).not.toContain("No sensor contact");
  });

  test("no target still reads as a sentence", () => {
    expect(describeCourse("Shadow", null, false).detail).toMatch(/^the target is pulling/);
  });
});
