// Звуки острова. Встроенные звуки синтезируются Web Audio на лету (без файлов):
// короткие, мягкие, с плавной атакой и затуханием. Свои звуки (WAV/MP3/OGG)
// играются через <audio>. У каждого типа свой канал — новый звук того же типа
// останавливает предыдущий, поэтому два сигнала одного типа не звучат разом.

import { ipc } from "../services/ipc";

export type Channel = "notification" | "timer" | "preview";

export interface SoundDef {
  id: string;
  label: string;
  /** Длина одного цикла (для повторяющегося сигнала таймера), с. */
  cycle: number;
  play: (ctx: AudioContext, out: AudioNode, t: number) => void;
}

interface ToneOpts {
  f: number;
  at?: number;
  dur: number;
  gain?: number;
  type?: OscillatorType;
  attack?: number;
  /** Конечная частота (глиссандо). */
  to?: number;
  glide?: number;
}

function tone(ctx: AudioContext, out: AudioNode, t0: number, o: ToneOpts) {
  const t = t0 + (o.at ?? 0);
  const osc = ctx.createOscillator();
  const g = ctx.createGain();
  osc.type = o.type ?? "sine";
  osc.frequency.setValueAtTime(o.f, t);
  if (o.to) osc.frequency.exponentialRampToValueAtTime(o.to, t + (o.glide ?? o.dur * 0.6));
  const peak = o.gain ?? 0.2;
  const attack = o.attack ?? 0.012;
  g.gain.setValueAtTime(0.0001, t);
  g.gain.exponentialRampToValueAtTime(peak, t + attack);
  g.gain.exponentialRampToValueAtTime(0.0001, t + o.dur);
  osc.connect(g).connect(out);
  osc.start(t);
  osc.stop(t + o.dur + 0.02);
}

/** Колокольный тон: основной + обертоны с разным затуханием. */
function bell(ctx: AudioContext, out: AudioNode, t: number, f: number, at = 0, dur = 0.9, gain = 0.16) {
  tone(ctx, out, t, { f, at, dur, gain });
  tone(ctx, out, t, { f: f * 2.01, at, dur: dur * 0.6, gain: gain * 0.35 });
  tone(ctx, out, t, { f: f * 3.02, at, dur: dur * 0.35, gain: gain * 0.15 });
}

export const NOTIFICATION_SOUNDS: SoundDef[] = [
  { id: "glass", label: "Стекло", cycle: 0.5, play: (c, o, t) => { tone(c, o, t, { f: 880, dur: 0.32, gain: 0.2 }); tone(c, o, t, { f: 1320, at: 0.09, dur: 0.34, gain: 0.18 }); } },
  { id: "drop", label: "Капля", cycle: 0.4, play: (c, o, t) => tone(c, o, t, { f: 1250, to: 520, glide: 0.11, dur: 0.28, gain: 0.24 }) },
  { id: "pop", label: "Щелчок", cycle: 0.3, play: (c, o, t) => { tone(c, o, t, { f: 480, to: 920, glide: 0.04, dur: 0.12, gain: 0.26, attack: 0.004 }); tone(c, o, t, { f: 1400, at: 0.035, dur: 0.06, gain: 0.05 }); } },
  { id: "bell", label: "Колокольчик", cycle: 1, play: (c, o, t) => bell(c, o, t, 1046.5, 0, 0.9, 0.17) },
  { id: "chord", label: "Аккорд", cycle: 0.7, play: (c, o, t) => [523.25, 659.25, 783.99].forEach((f, i) => tone(c, o, t, { f, at: i * 0.025, dur: 0.55, gain: 0.1, attack: 0.02 })) },
  { id: "pluck", label: "Струна", cycle: 0.5, play: (c, o, t) => { tone(c, o, t, { f: 784, dur: 0.22, gain: 0.2, type: "triangle", attack: 0.004 }); tone(c, o, t, { f: 1174.7, at: 0.08, dur: 0.3, gain: 0.17, type: "triangle", attack: 0.004 }); } },
  { id: "bubble", label: "Пузырёк", cycle: 0.45, play: (c, o, t) => { tone(c, o, t, { f: 380, to: 980, glide: 0.07, dur: 0.14, gain: 0.2 }); tone(c, o, t, { f: 600, to: 1300, at: 0.11, glide: 0.07, dur: 0.16, gain: 0.16 }); } },
  { id: "soft", label: "Мягкий", cycle: 0.6, play: (c, o, t) => tone(c, o, t, { f: 739.99, dur: 0.5, gain: 0.16, attack: 0.04 }) },
];

export const TIMER_SOUNDS: SoundDef[] = [
  { id: "radar", label: "Радар", cycle: 1.25, play: (c, o, t) => [0, 0.13, 0.26, 0.39].forEach((at) => tone(c, o, t, { f: 1180, at, dur: 0.09, gain: 0.16, type: "triangle", attack: 0.005 })) },
  { id: "beacon", label: "Маяк", cycle: 1.3, play: (c, o, t) => { tone(c, o, t, { f: 880, dur: 0.3, gain: 0.18, attack: 0.02 }); tone(c, o, t, { f: 659.25, at: 0.32, dur: 0.34, gain: 0.18, attack: 0.02 }); } },
  { id: "classic", label: "Будильник", cycle: 1, play: (c, o, t) => [0, 0.12, 0.24, 0.36].forEach((at) => tone(c, o, t, { f: 2000, at, dur: 0.07, gain: 0.06, type: "square", attack: 0.003 })) },
  { id: "chimes", label: "Перезвон", cycle: 1.8, play: (c, o, t) => [1046.5, 1318.5, 1568, 2093].forEach((f, i) => bell(c, o, t, f, i * 0.16, 0.8, 0.11)) },
  { id: "rise", label: "Подъём", cycle: 1.4, play: (c, o, t) => [523.25, 659.25, 783.99].forEach((f, i) => tone(c, o, t, { f, at: i * 0.17, dur: 0.22, gain: 0.17, type: "triangle", attack: 0.01 })) },
  { id: "marimba", label: "Маримба", cycle: 1.6, play: (c, o, t) => [783.99, 987.77, 1174.7, 987.77].forEach((f, i) => { tone(c, o, t, { f, at: i * 0.15, dur: 0.25, gain: 0.2, attack: 0.003 }); tone(c, o, t, { f: f * 4, at: i * 0.15, dur: 0.05, gain: 0.03, attack: 0.002 }); }) },
  { id: "pulse", label: "Пульс", cycle: 1.2, play: (c, o, t) => [0, 0.2, 0.4].forEach((at) => tone(c, o, t, { f: 440, at, dur: 0.16, gain: 0.22, attack: 0.02 })) },
  { id: "morning", label: "Утро", cycle: 2.4, play: (c, o, t) => [659.25, 783.99, 880, 1046.5].forEach((f, i) => tone(c, o, t, { f, at: i * 0.28, dur: 0.5, gain: 0.13, attack: 0.04 })) },
];

let ctx: AudioContext | null = null;
function audio(): AudioContext {
  ctx ??= new AudioContext();
  if (ctx.state === "suspended") ctx.resume().catch(() => {});
  return ctx;
}

const stops: Partial<Record<Channel, () => void>> = {};
const dataCache = new Map<string, string>();

export function stopSound(ch: Channel) {
  const s = stops[ch];
  delete stops[ch];
  s?.();
}

export function isPlaying(ch: Channel) {
  return !!stops[ch];
}

export function findSound(id: string): SoundDef | undefined {
  return [...NOTIFICATION_SOUNDS, ...TIMER_SOUNDS].find((s) => s.id === id);
}

/** Звук `id` (встроенный или custom:<файл>) в канал. loop — повторять до остановки или limitSecs. */
export async function playSound(ch: Channel, id: string, volume: number, opts: { loop?: boolean; limitSecs?: number; onEnd?: () => void } = {}) {
  stopSound(ch);
  const vol = Math.max(0, Math.min(1, volume));
  let stopped = false;
  let limitTimer: number | undefined;
  const finish = () => {
    if (stopped) return;
    stopped = true;
    window.clearTimeout(limitTimer);
    opts.onEnd?.();
  };

  if (id.startsWith("custom:")) {
    const file = id.slice(7);
    let url = dataCache.get(file);
    if (!url) {
      try {
        url = await ipc.soundData(file);
        dataCache.set(file, url);
      } catch {
        return playSound(ch, ch === "timer" ? "radar" : "glass", volume, opts);
      }
    }
    const el = new Audio(url);
    el.volume = vol;
    el.loop = !!opts.loop;
    el.onended = () => {
      if (stops[ch] === stop) delete stops[ch];
      finish();
    };
    const stop = () => {
      el.pause();
      el.src = "";
      finish();
    };
    stops[ch] = stop;
    el.play().catch(() => stop());
    if (opts.limitSecs) limitTimer = window.setTimeout(() => stops[ch] === stop && stopSound(ch), opts.limitSecs * 1000);
    return;
  }

  const def = findSound(id) ?? (ch === "timer" ? TIMER_SOUNDS[0] : NOTIFICATION_SOUNDS[0]);
  let c: AudioContext;
  try {
    c = audio();
  } catch {
    return;
  }
  const master = c.createGain();
  master.gain.value = vol;
  master.connect(c.destination);
  let interval: number | undefined;
  const schedule = () => def.play(c, master, c.currentTime + 0.02);
  schedule();
  if (opts.loop) {
    const gap = 0.35;
    interval = window.setInterval(schedule, (def.cycle + gap) * 1000);
  }
  const stop = () => {
    window.clearInterval(interval);
    const t = c.currentTime;
    master.gain.cancelScheduledValues(t);
    master.gain.setValueAtTime(master.gain.value, t);
    master.gain.linearRampToValueAtTime(0, t + 0.08);
    window.setTimeout(() => master.disconnect(), 150);
    finish();
  };
  stops[ch] = stop;
  if (opts.limitSecs) limitTimer = window.setTimeout(() => stops[ch] === stop && stopSound(ch), opts.limitSecs * 1000);
  else if (!opts.loop)
    window.setTimeout(() => {
      if (stops[ch] === stop) {
        delete stops[ch];
        master.disconnect();
        finish();
      }
    }, (def.cycle + 0.6) * 1000);
}
