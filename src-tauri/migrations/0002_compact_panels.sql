-- Меньше вкладок по умолчанию: в панели остаются Музыка, Центр управления,
-- Таймеры и Уведомления. Остальные карточки включаются в «Настройки → Карточки».
UPDATE widget_settings SET enabled = 0 WHERE card_id IN ('system', 'bluetooth', 'clipboard', 'apps', 'ai', 'integrations');
UPDATE widget_settings SET sort_order = 0 WHERE card_id = 'music';
UPDATE widget_settings SET sort_order = 1 WHERE card_id = 'quick';
UPDATE widget_settings SET sort_order = 2 WHERE card_id = 'timers';
UPDATE widget_settings SET sort_order = 3 WHERE card_id = 'notifications';
-- Положение «сверху по центру» теперь прилегает к краю экрана.
UPDATE settings SET value = replace(value, '"top":8.0', '"top":0.0') WHERE key = 'position';
