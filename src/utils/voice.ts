import { ipc } from "../services/ipc";

/** Проигрывает голосовую заметку (WAV из папки острова). */
export async function playVoice(name: string) {
  const url = await ipc.voiceData(name);
  await new Audio(url).play();
}

/** Приветствие по местному времени. */
export function greetingText(reason: string, now: Date = new Date()): string {
  const h = now.getHours();
  const part = h >= 5 && h < 12 ? "Доброе утро" : h >= 12 && h < 17 ? "Добрый день" : h >= 17 && h < 23 ? "Добрый вечер" : "Доброй ночи";
  return reason === "unlock" && h >= 9 && h < 21 ? "С возвращением" : part;
}
