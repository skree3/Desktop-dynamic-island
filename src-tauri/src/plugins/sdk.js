/* Desktop Island Plugin SDK v1.
 * Подключается плагином: <script src="island-sdk.js"></script>
 * Работает внутри sandbox-iframe; общается с панелью только через postMessage.
 */
(function () {
  "use strict";
  var PLUGIN_ID = "__PLUGIN_ID__";
  var seq = 0;
  var pending = {};
  var listeners = {};

  function call(method, params) {
    return new Promise(function (resolve, reject) {
      var id = ++seq;
      pending[id] = { resolve: resolve, reject: reject };
      parent.postMessage({ __island: 1, id: id, method: method, params: params || {} }, "*");
      setTimeout(function () {
        if (pending[id]) {
          delete pending[id];
          reject(new Error("Тайм-аут вызова " + method));
        }
      }, 15000);
    });
  }

  window.addEventListener("message", function (ev) {
    if (ev.source !== parent) return;
    var d = ev.data;
    if (!d || typeof d !== "object") return;
    if (d.__island && d.id && pending[d.id]) {
      var p = pending[d.id];
      delete pending[d.id];
      if (d.error) p.reject(Object.assign(new Error(d.error.message || "Ошибка"), { kind: d.error.kind }));
      else p.resolve(d.result);
    } else if (d.__island_event) {
      (listeners[d.__island_event] || []).forEach(function (fn) {
        try { fn(d.data); } catch (e) { console.error(e); }
      });
    }
  });

  window.addEventListener("error", function (e) {
    call("log", { level: "error", message: String(e.message || e) + " @" + (e.filename || "") + ":" + (e.lineno || 0) }).catch(function () {});
  });
  window.addEventListener("unhandledrejection", function (e) {
    call("log", { level: "error", message: "Unhandled rejection: " + String(e.reason && e.reason.message || e.reason) }).catch(function () {});
  });

  window.island = {
    pluginId: PLUGIN_ID,
    apiVersion: 1,
    info: function () { return call("app.info"); },
    log: function (message, level) { return call("log", { message: String(message), level: level || "info" }); },
    notify: function (opts) { return call("notify", opts); },
    settings: { get: function () { return call("settings.get"); } },
    storage: {
      get: function (key) { return call("storage.get", { key: key }); },
      set: function (key, value) { return call("storage.set", { key: key, value: value }); },
      remove: function (key) { return call("storage.set", { key: key, value: null }); }
    },
    system: { snapshot: function () { return call("system.snapshot"); } },
    media: {
      sessions: function () { return call("media.sessions"); },
      control: function (action) { return call("media.control", { action: action }); }
    },
    activity: {
      set: function (a) { return call("activity.set", a); },
      clear: function () { return call("activity.clear"); }
    },
    web: { open: function (url) { return call("web.open", { url: url }); } },
    on: function (event, fn) { (listeners[event] = listeners[event] || []).push(fn); },
    off: function (event, fn) { listeners[event] = (listeners[event] || []).filter(function (f) { return f !== fn; }); }
  };

  // Сообщаем панели высоту содержимого для автоматического размера карточки.
  function reportSize() {
    parent.postMessage({ __island_size: 1, height: document.documentElement.scrollHeight }, "*");
  }
  window.addEventListener("load", reportSize);
  if (window.ResizeObserver) new ResizeObserver(reportSize).observe(document.documentElement);
})();
