// Английский интерфейс. Тексты в коде написаны по-русски; при выборе English
// переводчик подменяет их в DOM (текст, title, placeholder, aria-label) по словарю
// en.json и следит за изменениями. Подписи вида «Громкость: 40%» переводятся по
// части до двоеточия.

import en from "./en.json";

const dict = en as Record<string, string>;
const ATTRS = ["title", "placeholder", "aria-label"];
let observer: MutationObserver | null = null;

function tr(s: string): string | null {
  const t = s.trim();
  if (!t || !/[А-Яа-яЁё]/.test(t)) return null;
  const exact = dict[t];
  if (exact) return s.replace(t, exact);
  const i = t.indexOf(":");
  if (i > 0) {
    const head = dict[t.slice(0, i)];
    const tail = dict[t.slice(i + 1).trim()];
    if (head || tail) {
      let out = s;
      if (head) out = out.replace(t.slice(0, i), head);
      if (tail) out = out.replace(t.slice(i + 1).trim(), tail);
      return out;
    }
  }
  const m = t.match(/^(.*?)\s(\S+)$/);
  if (m && dict[m[1]]) return s.replace(m[1], dict[m[1]]);
  return null;
}

function translateNode(node: Node) {
  if (node.nodeType === Node.TEXT_NODE) {
    const next = tr(node.nodeValue ?? "");
    if (next !== null && next !== node.nodeValue) node.nodeValue = next;
    return;
  }
  if (node.nodeType !== Node.ELEMENT_NODE) return;
  const el = node as Element;
  if (el.tagName === "TEXTAREA" || el.tagName === "SCRIPT" || el.tagName === "STYLE") return;
  for (const a of ATTRS) {
    const v = el.getAttribute(a);
    if (v) {
      const next = tr(v);
      if (next !== null && next !== v) el.setAttribute(a, next);
    }
  }
  el.childNodes.forEach(translateNode);
}

export function setLanguage(lang: string) {
  document.documentElement.lang = lang === "en" ? "en" : "ru";
  if (lang === "en" && !observer) {
    translateNode(document.body);
    observer = new MutationObserver((records) => {
      for (const r of records) {
        if (r.type === "characterData") translateNode(r.target);
        else if (r.type === "attributes") translateNode(r.target);
        else r.addedNodes.forEach(translateNode);
      }
    });
    observer.observe(document.body, { subtree: true, childList: true, characterData: true, attributes: true, attributeFilter: ATTRS });
  } else if (lang !== "en" && observer) {
    observer.disconnect();
    observer = null;
    // Вернуть русский проще всего перерисовкой окна.
    location.reload();
  }
}
