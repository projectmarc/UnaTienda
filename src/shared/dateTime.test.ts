import { describe, expect, it } from "vitest";
import { formatDateTime, formatTime } from "./dateTime";

describe("date and time formatting", () => {
  it("formats morning and afternoon times in 24-hour notation", () => {
    const morning = formatTime("2026-08-03T08:30:00");
    const afternoon = formatTime("2026-08-03T17:45:00");

    expect(morning).toBe("08:30");
    expect(afternoon).toBe("17:45");
    expect(`${morning} ${afternoon}`).not.toMatch(/\b(?:AM|PM)\b/i);
  });

  it("preserves the Argentine date presentation with a 24-hour time", () => {
    expect(formatDateTime("2026-08-03T08:30:00")).toBe(
      "3 de ago de 2026, 08:30",
    );
    expect(formatDateTime("2026-08-03T17:45:00")).toBe(
      "3 de ago de 2026, 17:45",
    );
  });
});
