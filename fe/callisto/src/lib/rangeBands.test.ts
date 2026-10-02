import {describe, expect, test} from "vitest";

import {bandName, formatBands} from "lib/range";

// The band edges the rules use, in metres.
const SHORT = 1_000_000;
const MEDIUM = 9_000_000;
const LONG = 20_000_000;
const VERY_LONG = 40_000_000;
const DISTANT = 80_000_000;

describe("range bands", () => {
  test("names the band a distance falls in", () => {
    expect(bandName(SHORT)).toBe("Short");
    expect(bandName(MEDIUM)).toBe("Medium");
    expect(bandName(LONG)).toBe("Long");
    expect(bandName(VERY_LONG)).toBe("Very Long");
    expect(bandName(DISTANT)).toBe("Distant");
  });

  // What a pilot is steering by: the band this burn ends the round in.
  test("shows the change when the round ends in a different band", () => {
    expect(formatBands({now: LONG, next: MEDIUM})).toBe("Long → Medium");
  });

  test("says it once when the band does not change", () => {
    expect(formatBands({now: LONG, next: LONG + 1})).toBe("Long");
  });
});
