// Данные раздела «Об авторе». Чтобы добавить соцсеть, допишите строку в `links`
// (иконки — из lucide-react), фото — замените src/assets/author.png.

import { Bug, Code2, Download, FolderGit2, Heart, Sparkles, Star, type LucideIcon } from "lucide-react";
import avatar from "../assets/author.png";

export interface AuthorLink {
  label: string;
  url: string;
  icon: LucideIcon;
}

const REPO = "https://github.com/skree3/Desktop-dynamic-island";

export const AUTHOR = {
  name: "skree3",
  handle: "skree3",
  avatar,
  bio: "Делаю Desktop Island — остров в стиле Dynamic Island для Windows и Linux.",
  bioEn: "I make Desktop Island — a Dynamic Island-style island for Windows and Linux.",
  badges: [
    { label: "Автор Desktop Island", icon: Sparkles },
    { label: "Открытый код", icon: Code2 },
    { label: "Сделано с душой", icon: Heart },
  ] as { label: string; icon: LucideIcon }[],
  links: [
    { label: "GitHub", url: "https://github.com/skree3", icon: FolderGit2 },
    { label: "Desktop Island на GitHub", url: REPO, icon: Star },
    { label: "Все версии", url: `${REPO}/releases`, icon: Download },
    { label: "Сообщить об ошибке", url: `${REPO}/issues`, icon: Bug },
  ] as AuthorLink[],
  contact: "",
};
