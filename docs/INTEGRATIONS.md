# Системные интеграции и фактический уровень поддержки

Если функция недоступна в окружении, интерфейс показывает «Недоступно» и причину — значения никогда не имитируются.

## Системные функции

| Функция | Windows 10/11 | Linux |
|---|---|---|
| Медиасессии | SMTC (`GlobalSystemMediaTransportControlsSessionManager`): метаданные, обложка, прогресс, пауза, треки, перемотка | MPRIS (D-Bus): то же + громкость конкретного плеера |
| Громкость | Core Audio (`IAudioEndpointVolume`): уровень, mute, список устройств. Смена устройства по умолчанию — нет документированного API → кнопка «Параметры → Звук» | `pactl` (PulseAudio / PipeWire-pulse): уровень, mute, **выбор устройства**; запасной вариант `wpctl` |
| Яркость | WMI `WmiMonitorBrightness(Methods)` для встроенных панелей + DDC/CI (Dxva2) для внешних мониторов | `/sys/class/backlight` + logind `SetBrightness` (без root). DDC/CI — не поддерживается |
| Wi-Fi | `Windows.Devices.Radios` (вкл/выкл с запросом доступа) + `NetworkInformation` (SSID) | NetworkManager (`WirelessEnabled`, через polkit) |
| Bluetooth | Radios (питание адаптера) + `DeviceInformation` (сопряжённые/подключённые Classic и LE) | BlueZ (`Adapter1.Powered`, `Device1`, `Battery1`) |
| Режим «в самолёте» | Только чтение (все радиомодули выключены); переключение — `ms-settings:network-airplanemode` | Только чтение (rfkill); переключение — в настройках окружения |
| Энергосбережение | Только чтение (`SystemStatusFlag`); публичного API включения нет | power-profiles-daemon (`power-saver` ↔ `balanced`) |
| Батарея | `GetSystemPowerStatus` | `/sys/class/power_supply` |
| Выключение/перезагрузка | `ExitWindowsEx` (без `EWX_FORCE`, с привилегией `SeShutdownPrivilege`) | logind `PowerOff/Reboot(interactive=true)` |
| CPU / RAM / диски / сеть | sysinfo | sysinfo |
| GPU | NVML (NVIDIA: загрузка, температура, память); иначе PDH `GPU Engine 3D` (любая видеокарта, только загрузка) | NVML; иначе sysfs `amdgpu` (загрузка, температура); Intel — недоступно |
| Температура CPU | sysinfo/WMI — обычно **недоступно** без драйвера/прав администратора | hwmon (k10temp, coretemp, zenpower) |
| Уведомления других приложений | `UserNotificationListener` — доступность зависит от сборки Windows и разрешения «Доступ к уведомлениям». На тестовой машине (Windows 11 26300) — **доступно**. Запасной путь для Telegram и форков, которые рисуют свои всплывающие окна: рост счётчика непрочитанных в заголовке окна, если за 3 с не пришло системное уведомление | Мониторинг `org.freedesktop.Notifications.Notify`, `org.gtk.Notifications.AddNotification` и портала `org.freedesktop.portal.Notification` (BecomeMonitor). Отправитель портала определяется по PID (Flatpak, Snap), дубли склеиваются. Действия уведомлений вызвать нельзя; во Flatpak — недоступно |
| Буфер обмена | arboard + `GetClipboardSequenceNumber`; маркеры менеджеров паролей (`ExcludeClipboardContentFromMonitorProcessing`, `CanIncludeInClipboardHistory=0`) | arboard (X11; Wayland — протокол data-control: KDE, wlroots; GNOME — частично через XWayland) |
| Игры (игровой режим) | `GetForegroundWindow` + прямоугольник монитора, `SHQueryUserNotificationState` (эксклюзивный D3D), пути игр, лаунчер-родитель процесса; браузеры, плееры, офис, мессенджеры не считаются играми | EWMH `_NET_WM_STATE_FULLSCREEN` (X11/XWayland); для нативных Wayland-окон — по процессам (gamescope, Proton/Wine, steamapps) |
| Поиск приложений | `shell:AppsFolder` (классические, Store/MSIX, PWA) + иконки `IShellItemImageFactory` | Desktop Entry (XDG, Flatpak, Snap) + иконки темы hicolor |
| Глобальные клавиши | tauri-plugin-global-shortcut | X11 / XWayland. Нативный Wayland — назначьте сочетание в DE на `desktop-island --toggle` и т.п. |
| Позиция «поверх окон» | да | X11 — да; Wayland — через XWayland (`GDK_BACKEND=x11` выставляется автоматически, отключается `DESKTOP_ISLAND_NATIVE_WAYLAND=1`) |
| Прокси | Чтение `Internet Settings` из реестра (без изменений) | Чтение переменных окружения и GNOME gsettings (без изменений) |

## Приложения

Уровни: **Полноценное управление**, **Частичная интеграция**, **Только уведомления**, **Только запуск приложения**, **Только веб-версия**. Уровень «Только запуск» для мессенджеров на Windows автоматически повышается до «Только уведомления», если API уведомлений доступен.

| Приложение | Windows | Linux | Комментарий |
|---|---|---|---|
| Spotify | Полноценное | Полноценное | SMTC / MPRIS |
| YouTube, YouTube Music, SoundCloud | Частичная | Частичная | Через медиасессию браузера; управлять вкладками нельзя |
| Яндекс Музыка | Частичная | Частичная | Приложение/веб публикуют медиасессию |
| VLC | Только запуск* | Полноценное | *VLC 3.x под Windows не публикует SMTC; если версия публикует — управление появится автоматически |
| Chrome / Edge / Firefox | Частичная | Частичная | Медиасессия активной вкладки |
| Telegram, Discord | Только запуск / Уведомления | Только уведомления | Протоколы `tg://`, `discord://` |
| WhatsApp | Только запуск / Уведомления | Только веб-версия | Нет официального клиента Linux |
| Steam | Частичная | Частичная | Запуск, `steam://`, распознавание игр для игрового режима |
| Happ, INCY | Частичная (только чтение) | Частичная (только чтение) | Статус процесса и системного прокси. Документированного API управления подключением нет — управление **не реализовано**, системный прокси и VPN не изменяются |
| Claude, ChatGPT | Частичная | Только веб-версия | Запуск установленного приложения + веб-мини-окно |
| DeepSeek, Gemini | Только веб-версия | Только веб-версия | Изолированное мини-окно |

Список не закрыт: любые установленные программы находятся поиском автоматически, новые программы и веб-сервисы добавляются вручную (Настройки → Интеграции), при этом показываются только реально доступные способы интеграции.

## Нейросети в мини-окнах
Каждый сервис открывается в отдельном окне WebView с собственным каталогом данных (`%APPDATA%/app.desktopisland/webdata/<сервис>`), поэтому cookies и авторизация не смешиваются. Окна не имеют доступа к IPC приложения. Ограничения сайтов и авторизации не обходятся. Кнопка «Выйти» удаляет данные сервиса.

## Проверено на реальной системе (`desktop-island --selftest`)
Windows 11 Pro 10.0.26300, AMD Ryzen 7 4800H, NVIDIA RTX 2060, 16 ГБ RAM:

```
media::list_sessions        Ok([])                       16 ms (нет активного плеера в момент теста)
audio::state                Динамики (Realtek), 3 устройства            10 ms
brightness::state           WMI, встроенный дисплей 70%                 21 ms
network::wifi_state         Wi-Fi вкл., не подключён                    32 ms
bluetooth::state            адаптер есть, 6 сопряжённых устройств       69 ms
power::state                батарея 100%, от сети, экономия выкл.        0 ms
apps::scan                  251 приложение                              657 ms
apps::icon                  PNG получен                                 ~40 ms
notifications::open         UserNotificationListener доступен           9 ms
gpu::PdhGpu                 17.7 %                                      1113 ms (включая паузу 0.5 с)
nvml                        NVIDIA GeForce RTX 2060                     36 ms
sysinfo components (temps)  []  → температура CPU «Недоступно»
```
