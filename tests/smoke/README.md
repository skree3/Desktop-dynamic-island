# Смоук-тесты живого приложения

Скрипты выполняются внутри окна панели через Chrome DevTools Protocol WebView2.

1. Запустите приложение с отладкой WebView2:
   ```powershell
   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222"; npm run tauri dev
   ```
2. Откройте `http://127.0.0.1:9222`, выберите страницу `index.html?window=island` и вставьте содержимое
   `ipc-smoke.js` или `plugin-smoke.js` в консоль DevTools (или выполните через `Runtime.evaluate`).

* `ipc-smoke.js` — все команды «только чтение» + негативные проверки (поддельный токен выключения, неизвестный раздел настроек, `javascript:` URL, несуществующий плагин).
* `plugin-smoke.js` — установка примера плагина, отказ без подтверждения прав, API, отказ по неразрешённому праву, валидация настроек.

Системные адаптеры без UI: `desktop-island --selftest`.
