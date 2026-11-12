import { describe, expect, it } from "vitest";
import { formatBytes, formatClock, formatStopwatch, hexToRgba, mediaPosition, parseDuration, formatPercent } from "../src/utils/format";
import { pickCompact } from "../src/stores/island";

describe("format", () => {
  it("bytes", () => {
    expect(formatBytes(0)).toBe("0 Б");
    expect(formatBytes(1536)).toBe("1.5 КБ");
    expect(formatBytes(null)).toBe("—");
  });

  it("clock", () => {
    expect(formatClock(75_000)).toBe("1:15");
    expect(formatClock(3_725_000)).toBe("1:02:05");
    expect(formatStopwatch(61_234)).toBe("1:01.2");
  });

  it("unavailable sensors are not faked", () => {
    expect(formatPercent(null)).toBe("Недоступно");
  });

  it("colors", () => {
    expect(hexToRgba("#000000", 0.5)).toBe("rgba(0, 0, 0, 0.500)");
    expect(hexToRgba("rgba(255, 255, 255, 0.5)", 0.5)).toBe("rgba(255, 255, 255, 0.250)");
  });

  it("media interpolation", () => {
    expect(mediaPosition(1000, 0, true, 5000, 2000)).toBe(3000);
    expect(mediaPosition(1000, 0, false, 5000, 2000)).toBe(1000);
    expect(mediaPosition(4000, 0, true, 5000, 9000)).toBe(5000);
  });

  it("duration parsing", () => {
    expect(parseDuration("5")).toBe(300_000);
    expect(parseDuration("1:30")).toBe(90_000);
    expect(parseDuration("1:00:00")).toBe(3_600_000);
    expect(parseDuration("90s")).toBe(90_000);
    expect(parseDuration("2 мин")).toBe(120_000);
    expect(parseDuration("abc")).toBeNull();
  });
});

describe("compact priority", () => {
  const base = { subtitle: null, icon: null, progress: null, updatedAt: 0 };
  it("timer beats music, music beats idle", () => {
    const acts = [
      { ...base, id: "music", kind: "music", priority: "normal" as const, title: "Song" },
      { ...base, id: "timer:1", kind: "countdown", priority: "high" as const, title: "Tea" },
    ];
    expect(pickCompact(acts)?.id).toBe("timer:1");
    expect(pickCompact([acts[0]])?.id).toBe("music");
    expect(pickCompact([])).toBeNull();
  });
});
