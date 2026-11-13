#!/usr/bin/env bash
# Сборка установщиков Desktop Island для Linux (.deb, .rpm, .AppImage).
# Запуск из корня проекта:  bash scripts/build-linux.sh
# Скрипт сам ставит системные зависимости (apt / dnf / pacman / zypper),
# Rust (rustup) и Node.js 20+, затем собирает установщики в ./dist-linux.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"

log() { printf '\n\033[1;34m==> %s\033[0m\n' "$*"; }

SUDO=""
if [ "$(id -u)" -ne 0 ]; then SUDO="sudo"; fi

log "Установка системных зависимостей"
if command -v apt-get >/dev/null 2>&1; then
  $SUDO apt-get update
  $SUDO apt-get install -y build-essential curl wget file pkg-config libssl-dev \
    libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libasound2-dev \
    rpm xdg-utils
elif command -v dnf >/dev/null 2>&1; then
  $SUDO dnf install -y webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel librsvg2-devel \
    openssl-devel curl wget file rpm-build xdg-utils
  $SUDO dnf group install -y "c-development" || $SUDO dnf groupinstall -y "Development Tools"
elif command -v pacman >/dev/null 2>&1; then
  $SUDO pacman -S --needed --noconfirm webkit2gtk-4.1 base-devel curl wget file openssl \
    appmenu-gtk-module libappindicator-gtk3 librsvg xdotool xdg-utils
elif command -v zypper >/dev/null 2>&1; then
  $SUDO zypper install -y webkit2gtk3-devel libopenssl-devel curl wget file libappindicator3-1 \
    librsvg-devel rpm-build xdg-utils
  $SUDO zypper install -y -t pattern devel_basis
else
  echo "Неизвестный пакетный менеджер — установите зависимости вручную (см. README.md)." >&2
fi

if ! command -v cargo >/dev/null 2>&1; then
  log "Установка Rust (rustup)"
  if ! curl --proto '=https' --tlsv1.2 -sSf --max-time 30 https://sh.rustup.rs | sh -s -- -y --profile minimal; then
    # sh.rustup.rs бывает недоступен (блокировки, VPN) — берём rustup-init напрямую.
    log "sh.rustup.rs недоступен, скачиваю rustup-init с static.rust-lang.org"
    curl --proto '=https' --tlsv1.2 -sSf --retry 3 -o /tmp/rustup-init \
      "https://static.rust-lang.org/rustup/dist/$(uname -m)-unknown-linux-gnu/rustup-init"
    chmod +x /tmp/rustup-init
    /tmp/rustup-init -y --profile minimal
    rm -f /tmp/rustup-init
  fi
fi
# shellcheck disable=SC1091
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

NODE_MAJOR=0
if command -v node >/dev/null 2>&1; then NODE_MAJOR="$(node -p 'process.versions.node.split(".")[0]')"; fi
if [ "$NODE_MAJOR" -lt 20 ]; then
  NODE_VER="v22.12.0"
  NODE_DIR="$HOME/.local/node-$NODE_VER"
  if [ ! -x "$NODE_DIR/bin/node" ]; then
    log "Установка Node.js $NODE_VER"
    mkdir -p "$NODE_DIR"
    curl -fL --retry 5 --retry-delay 3 --retry-all-errors -o /tmp/node.tar.xz \
      "https://nodejs.org/dist/$NODE_VER/node-$NODE_VER-linux-x64.tar.xz"
    tar -xJf /tmp/node.tar.xz -C "$NODE_DIR" --strip-components=1
    rm -f /tmp/node.tar.xz
  fi
  export PATH="$NODE_DIR/bin:$PATH"
fi
node --version

log "Установка npm-зависимостей"
npm ci || npm install

log "Сборка (release)"
BUNDLES="deb,rpm,appimage"
npx tauri build --bundles "$BUNDLES"

log "Копирование установщиков в ./dist-linux"
mkdir -p "$ROOT/dist-linux"
find "$ROOT/src-tauri/target/release/bundle" -maxdepth 2 -type f \( -name '*.deb' -o -name '*.rpm' -o -name '*.AppImage' \) -exec cp -v {} "$ROOT/dist-linux/" \;

log "Готово"
ls -lh "$ROOT/dist-linux"
cat <<'EOF'

Установка:
  Ubuntu/Debian:  sudo apt install ./dist-linux/*.deb
  Fedora:         sudo dnf install ./dist-linux/*.rpm
  Любой дистрибутив: chmod +x dist-linux/*.AppImage && ./dist-linux/*.AppImage
Проверка адаптеров на этой системе:  desktop-island --selftest
EOF
