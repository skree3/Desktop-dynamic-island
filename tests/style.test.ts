import { describe, expect, it } from "vitest";
import { buildPalettes, contrast, mix } from "../src/utils/style";

describe("свой стиль", () => {
  it("смешивание цветов", () => {
    expect(mix("#000000", "#ffffff", 0.5)).toBe("#808080");
    expect(mix("#ff0000", "#0000ff", 0)).toBe("#ff0000");
  });
  it("палитры читаемы", () => {
    for (const accent of ["#f0a64a", "#4c9aff", "#ffe600", "#1a1a1a"]) {
      const { dark, light } = buildPalettes(accent, "#2b6cb0", "#ff66aa");
      expect(contrast(dark.text, dark.bg)).toBeGreaterThan(10);
      expect(contrast(light.text, light.bg)).toBeGreaterThan(10);
      expect(contrast(light.accent, light.bg)).toBeGreaterThanOrEqual(4.5);
      expect(contrast(dark.accentText, dark.accent)).toBeGreaterThanOrEqual(3);
    }
  });
});
