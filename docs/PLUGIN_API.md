# Плагины (API v1)

Плагин добавляет свою карточку, строку в свёрнутом острове, уведомления и ссылки на сайты. Пример: [`examples/plugins/system-pulse`](../examples/plugins/system-pulse).

## Безопасность

- Плагин работает в изолированном iframe: без доступа к сети, файлам, окну приложения и IPC.
- Каждый вызов проверяется: плагин включён и пользователь выдал нужное разрешение.
- Включить плагин можно только после подтверждения его разрешений.

## Структура

```
my-plugin/
├── manifest.json
├── index.html
└── main.js
```

```json
{
  "id": "com.example.my-plugin",
  "name": "Мой плагин",
  "version": "1.0.0",
  "apiVersion": 1,
  "entry": "index.html",
  "permissions": ["notifications", "storage"],
  "card": { "title": "Мой плагин", "height": 180 },
  "settings": [{ "key": "city", "type": "string", "label": "Город", "default": "Москва" }]
}
```

`id` — латиница, цифры, `.` `_` `-`; `version` — semver; `apiVersion` — всегда `1`. Типы настроек: `string`, `number`, `boolean`, `select`.

## Разрешения

| Разрешение | Что даёт |
|---|---|
| `notifications` | `island.notify()` |
| `storage` | `island.storage.get/set/remove` |
| `system.read` | `island.system.snapshot()` — процессор, память, диски, сеть |
| `media.read` / `media.control` | Список плееров / пауза и переключение треков |
| `activity` | `island.activity.set/clear` — своя строка в свёрнутом острове |
| `web.open` | `island.web.open(url)` — только https и домены из `allowedHosts` |

Без разрешений доступны `island.info()`, `island.log()` и `island.settings.get()`.

## SDK

Подключите `<script src="island-sdk.js"></script>` — файл отдаёт само приложение. Все методы возвращают `Promise`.

```js
await island.notify({ title: "Готово", body: "Текст", actions: [{ id: "open", label: "Открыть" }] });
await island.storage.set("counter", 5);
await island.activity.set({ title: "Сборка", subtitle: "42%", progress: 0.42 });
island.on("notificationAction", ({ action }) => { /* … */ });
```

## Установка

«Настройки → Подключения → Плагины» → «Установить из папки…». После установки включите плагин и подтвердите разрешения. Обновление — повторная установка с большей версией.
