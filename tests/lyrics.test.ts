import { describe, expect, it } from "vitest";
import { currentLine } from "../src/hooks/useLyrics";

describe("currentLine", () => {
  const lines: [number, string][] = [
    [1000, "Первая"],
    [5000, ""],
    [8000, "Третья"],
  ];
  it("до первой строки ничего не показывает", () => expect(currentLine(lines, 500)).toBeNull());
  it("берёт последнюю начавшуюся строку", () => {
    expect(currentLine(lines, 1000)).toBe("Первая");
    expect(currentLine(lines, 4999)).toBe("Первая");
    expect(currentLine(lines, 9000)).toBe("Третья");
  });
  it("пустая строка — проигрыш, показываем название трека", () => expect(currentLine(lines, 6000)).toBeNull());
});
