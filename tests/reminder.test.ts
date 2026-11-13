import { describe, expect, it } from "vitest";
import { parseReminder, reminderLabel } from "../src/utils/reminder";

const now = new Date(2026, 10, 13, 15, 0, 0); // пятница 13 ноября 2026, 15:00

describe("parseReminder", () => {
  it("через N минут и часов", () => {
    expect(parseReminder("через 20 мин позвонить", now)).toEqual({ at: now.getTime() + 20 * 60_000, text: "позвонить" });
    expect(parseReminder("выключить духовку через 2 часа", now)?.at).toBe(now.getTime() + 2 * 3_600_000);
    expect(parseReminder("через полчаса чай", now)?.text).toBe("чай");
  });
  it("время сегодня и перенос на завтра", () => {
    const r = parseReminder("позвонить маме в 18:30", now)!;
    expect(new Date(r.at).getHours()).toBe(18);
    expect(new Date(r.at).getDate()).toBe(13);
    expect(r.text).toBe("позвонить маме");
    const early = parseReminder("в 9:00 зарядка", now)!;
    expect(new Date(early.at).getDate()).toBe(14);
  });
  it("завтра и время суток", () => {
    const r = parseReminder("завтра в 9 забрать посылку", now)!;
    expect(new Date(r.at).getDate()).toBe(14);
    expect(new Date(r.at).getHours()).toBe(9);
    expect(r.text).toBe("забрать посылку");
    expect(new Date(parseReminder("в 7 вечера кино", now)!.at).getHours()).toBe(19);
  });
  it("обычный текст с числами — не напоминание", () => {
    expect(parseReminder("купить 3 яблока", now)).toBeNull();
    expect(parseReminder("идея для проекта", now)).toBeNull();
  });
  it("подпись времени", () => {
    expect(reminderLabel(new Date(2026, 10, 13, 18, 30).getTime(), now)).toBe("18:30");
    expect(reminderLabel(new Date(2026, 10, 14, 9, 0).getTime(), now)).toBe("завтра 9:00");
  });
});
