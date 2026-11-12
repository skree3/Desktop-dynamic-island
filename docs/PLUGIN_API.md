# Plugin API v1

Плагины расширяют Desktop Island новыми карточками, источниками данных (активности в компактной панели), уведомлениями, веб-приложениями в поиске и действиями. Пример: [`examples/plugins/system-pulse`](../examples/plugins/system-pulse).

## Модель безопасности

| Механизм | Что даёт |
|---|---|
| `<iframe sandbox="allow-scripts">` без `allow-same-origin` | Непрозрачный origin: нет доступа к DOM панели, cookies, localStorage приложения и IPC Tauri |
| Протокол `plugin://` + CSP | `connect-src 'none'` — сеть запрещена; ресурсы загружаются только из каталога плагина; `frame-src 'none'`, `form-action 'none'` |
| Брокер `plugin_call` (Rust) | Каждый вызов проверяется: плагины включены глобально, плагин включён, разрешение выдано пользователем; лимит 30 вызовов/с |
| Подтверждение прав | Включение плагина невозможно без явного подтверждения списка разрешений. При обновлении новые разрешения требуют повторного подтверждения |
| Копирование при установке | Файлы копируются в каталог данных; символические ссылки игнорируются; лимит 20 МБ / 500 файлов; пути проверяются на выход за пределы каталога |
| Журнал | Ошибки плагина (`window.onerror`, `unhandledrejection`, `island.log`) и попытки вызова без разрешения пишутся в журнал событий (категория `plugin` / `permission`) |

Плагин **не может**: читать файлы, выполнять команды, обращаться к сети, изменять системные настройки, читать буфер обмена или уведомления других приложений.

## Структура

```
my-plugin/
├── manifest.json
├── index.html      ← entry
├── main.js
└── style.css
```

### manifest.json

```json
{
  "id": "com.example.my-plugin",
  "name": "Мой плагин",
  "version": "1.0.0",
  "apiVersion": 1,
  "minAppVersion": "0.1.0",
  "description": "Описание",
  "author": "Автор",
  "entry": "index.html",
  "permissions": ["notifications", "storage"],
  "card": { "title": "Мой плагин", "height": 180 },
  "settings": [
    { "key": "city", "type": "string", "label": "Город", "default": "Москва" },
    { "key": "units", "type": "select", "label": "Единицы", "default": "°C", "options": ["°C", "°F"] },
    { "key": "compact", "type": "boolean", "label": "Компактный вид", "default": false },
    { "key": "limit", "type": "number", "label": "Лимит", "default": 10 }
  ],
  "apps": [{ "name": "Мой сервис", "url": "https://example.com" }],
  "allowedHosts": ["example.com"]
}
```

| Поле | Обязательно | Правила |
|---|---|---|
| `id` | да | `[a-z0-9._-]`, 3–64 символа |
| `version` | да | semver; обновление на меньшую версию запрещено |
| `apiVersion` | да | должна равняться `1` (иначе — «несовместим») |
| `minAppVersion` | нет | semver; проверяется при установке |
| `entry` | нет (`index.html`) | относительный путь к HTML внутри плагина |
| `permissions` | нет | только из списка ниже |
| `card` | нет | `title` 1–48 символов, `height` 60–600 px (максимум; фактическая высота — по содержимому) |
| `settings` | нет | ≤ 32 настроек; типы `string`, `number`, `boolean`, `select` |
| `allowedHosts` | нет | домены для `web.open` |

### Разрешения

| Разрешение | Методы |
|---|---|
| `notifications` | `island.notify()` (приоритет `low`/`normal`/`high`; `critical` плагинам недоступен) |
| `storage` | `island.storage.get/set/remove` (≤ 64 КБ на значение, ≤ 200 ключей) |
| `system.read` | `island.system.snapshot()` |
| `media.read` | `island.media.sessions()` (без обложек и внутренних идентификаторов) |
| `media.control` | `island.media.control("playPause" \| "next" \| "previous")` |
| `activity` | `island.activity.set/clear` — показ своей строки в компактной панели |
| `web.open` | `island.web.open(url)` — только https и только `allowedHosts` |

Без разрешений доступны: `island.info()`, `island.log()`, `island.settings.get()`.

## SDK

Подключите в HTML: `<script src="island-sdk.js"></script>` (файл отдаётся приложением автоматически, класть его в плагин не нужно). Все методы возвращают `Promise`; ошибки имеют поле `kind` (`permission_denied`, `invalid_input`, `unsupported`, …).

```js
await island.info();                         // { apiVersion, appVersion, os, pluginId, permissions }
await island.log("сообщение", "warn");       // в журнал событий
const s = await island.settings.get();       // значения настроек из manifest (с умолчаниями)

await island.notify({ title: "Готово", body: "Текст", priority: "high",
                      actions: [{ id: "open", label: "Открыть" }] });   // → { id }
await island.storage.set("counter", 5);
await island.storage.get("counter");         // 5
await island.storage.remove("counter");

await island.system.snapshot();              // { cpu, ramUsed, ramTotal, gpu: {...}, cpuTemp, disks, netRxBps, power, ... }
await island.media.sessions();               // [{ id, app, title, artist, status, durationMs, positionMs }]
await island.media.control("playPause");
await island.activity.set({ title: "Сборка", subtitle: "42%", progress: 0.42 });
await island.activity.clear();
await island.web.open("https://example.com/page");

island.on("settingsChanged", () => { /* перечитать настройки */ });
island.on("notificationAction", ({ notificationId, action }) => { /* … */ });
```

Высота карточки подстраивается под содержимое автоматически (SDK сообщает `scrollHeight`), но не больше `card.height`.

## Установка, обновление, удаление

Настройки → «Плагины» → «Установить из папки…» и выберите каталог с `manifest.json`. После установки плагин выключен — включите его и подтвердите разрешения. Обновление — повторная установка из папки с большей версией (разрешения сохраняются, новые требуют подтверждения). Удаление стирает файлы и настройки плагина.

## Версионирование

`apiVersion` — мажорная версия контракта SDK и брокера. Обратно несовместимые изменения увеличивают её; приложение отказывается устанавливать плагины с другой версией и показывает причину.
