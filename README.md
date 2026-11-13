# Desktop Island

Остров сверху экрана в стиле Dynamic Island для Windows и Linux. В свёрнутом виде показывает время, трек, таймер или звонок, по нажатию раскрывается в панель.

## Скачать

| Система | Файл |
|---|---|
| Windows 10 и 11 | [Установщик](https://github.com/skree3/Desktop-dynamic-island/releases/latest/download/DesktopIsland-windows-setup.exe) · [MSI](https://github.com/skree3/Desktop-dynamic-island/releases/latest/download/DesktopIsland-windows.msi) · [Без установки](https://github.com/skree3/Desktop-dynamic-island/releases/latest/download/DesktopIsland-windows-portable.zip) |
| Ubuntu, Debian, Mint | [DEB](https://github.com/skree3/Desktop-dynamic-island/releases/latest/download/DesktopIsland-linux-x64.deb) |
| Fedora, openSUSE | [RPM](https://github.com/skree3/Desktop-dynamic-island/releases/latest/download/DesktopIsland-linux-x64.rpm) |
| Любой Linux | [AppImage](https://github.com/skree3/Desktop-dynamic-island/releases/latest/download/DesktopIsland-linux-x64.AppImage) · [Flatpak](https://github.com/skree3/Desktop-dynamic-island/releases/latest/download/DesktopIsland-linux-x64.flatpak) |

Новые версии приложение предлагает установить само.

## Возможности

- **Уведомления** всех программ в острове, сгруппированные по приложениям. Системные баннеры Windows не мешают.
- **Телефон** через бесплатное приложение [KDE Connect](https://kdeconnect.kde.org): уведомления и ответы на них, звонки, заряд, общий буфер обмена, передача файлов, «Найти телефон».
- **Музыка**: обложка, управление, текст песни строкой или на весь остров.
- **Звонки, микрофон и камера**: длительность звонка, кнопка микрофона, индикаторы.
- **Заметки и напоминания** текстом и голосом: «позвонить маме в 18:30». Голос превращается в текст без интернета.
- **Календарь** по ICS-ссылке, **Помодоро**, таймеры и секундомер.
- **Полка** для файлов: перетащите файл на остров. Снимок области экрана, текст с картинки и перевод.
- **Звук**: устройство вывода, громкость каждой программы, Bluetooth-наушники с зарядом.
- **Центр управления**: Wi-Fi, Bluetooth, яркость, загрузки, заряд батареи.
- **Внешний вид**: темы, свой стиль из 1–3 цветов, остров у края или плавающий, обои и прозрачность окна настроек, русский и английский.

## Как пользоваться

Нажмите на остров, чтобы открыть, и мимо него, чтобы закрыть. Колесо мыши листает разделы, остров можно перетащить. Правая кнопка открывает настройки.

| Действие | Сочетание |
|---|---|
| Показать или скрыть остров | Ctrl+Alt+I |
| Открыть панель | Ctrl+Alt+E |
| Центр управления | Ctrl+Alt+Q |
| Пауза музыки | Ctrl+Alt+P |
| Не беспокоить | Ctrl+Alt+D |

## Если что-то не так

- **«Система Windows защитила ваш компьютер»**: нажмите «Подробнее» → «Выполнить в любом случае».
- **Telegram присылает только «Новое сообщение»**: в Telegram «Настройки → Уведомления и звуки» включите «Использовать системные уведомления».
- **AppImage не запускается**: `sudo apt install libfuse2t64` (на старых Ubuntu `libfuse2`).
- **Wayland**: назначьте сочетание клавиш на команду `desktop-island --toggle`.
- Остальное — в [Issues](https://github.com/skree3/Desktop-dynamic-island/issues), приложите вывод `desktop-island --selftest`.

## Приватность

Данные хранятся только на компьютере. В интернет приложение обращается для проверки обновлений, текста песен (lrclib.net) и календаря, а для перевода и загрузки модели распознавания речи — только по вашей кнопке. Всё это отключается в настройках.

## Разработка

Rust (Tauri) и React. `npm install`, затем `npm run tauri dev`. Сборка под Linux: `bash scripts/build-linux.sh`. Релиз собирается на GitHub по тегу `vX.Y.Z`. Документация: [устройство](docs/ARCHITECTURE.md), [поддержка систем](docs/INTEGRATIONS.md), [плагины](docs/PLUGIN_API.md).
