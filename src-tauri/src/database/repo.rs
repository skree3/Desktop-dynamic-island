//! Общие операции с таблицами настроек, карточек, тем, профилей и горячих клавиш.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::{now_ms, Database};
use crate::core::error::AppResult;
use crate::core::settings::{Settings, SECTIONS};

impl Database {
    pub fn load_settings(&self) -> AppResult<Settings> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut obj = serde_json::to_value(Settings::default())?;
        for row in rows.flatten() {
            let (key, value) = row;
            if !SECTIONS.contains(&key.as_str()) {
                continue;
            }
            match serde_json::from_str::<serde_json::Value>(&value) {
                Ok(v) => {
                    if let Some(slot) = obj.get_mut(&key) {
                        crate::core::settings::merge_json(slot, v);
                    }
                }
                Err(e) => tracing::warn!(section = %key, error = %e, "повреждённый раздел настроек пропущен"),
            }
        }
        let mut settings: Settings = serde_json::from_value(obj).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "настройки не прочитаны, используются значения по умолчанию");
            Settings::default()
        });
        settings.sanitize();
        Ok(settings)
    }

    pub fn save_section(&self, settings: &Settings, section: &str) -> AppResult<()> {
        let value = settings.section_value(section)?;
        self.conn().execute(
            "INSERT INTO settings(key, value, updated_at) VALUES(?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![section, value.to_string(), now_ms()],
        )?;
        Ok(())
    }

    pub fn save_all_settings(&self, settings: &Settings) -> AppResult<()> {
        for s in SECTIONS {
            self.save_section(settings, s)?;
        }
        Ok(())
    }

    pub fn has_settings(&self) -> bool {
        self.conn()
            .query_row("SELECT count(*) FROM settings", [], |r| r.get::<_, i64>(0))
            .map(|n| n > 0)
            .unwrap_or(false)
    }

    // ---- карточки ----

    pub fn list_widgets(&self) -> AppResult<Vec<WidgetSetting>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT card_id, enabled, sort_order, priority, config FROM widget_settings ORDER BY sort_order")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(WidgetSetting {
                    card_id: r.get(0)?,
                    enabled: r.get::<_, i64>(1)? != 0,
                    sort_order: r.get(2)?,
                    priority: r.get(3)?,
                    config: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or(serde_json::json!({})),
                })
            })?
            .flatten()
            .collect();
        Ok(rows)
    }

    pub fn upsert_widget(&self, w: &WidgetSetting) -> AppResult<()> {
        self.conn().execute(
            "INSERT INTO widget_settings(card_id, enabled, sort_order, priority, config) VALUES(?1,?2,?3,?4,?5)
             ON CONFLICT(card_id) DO UPDATE SET enabled=excluded.enabled, sort_order=excluded.sort_order,
             priority=excluded.priority, config=excluded.config",
            params![w.card_id, w.enabled as i64, w.sort_order, w.priority, w.config.to_string()],
        )?;
        Ok(())
    }

    // ---- темы ----

    pub fn list_themes(&self) -> AppResult<Vec<ThemeRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT id, name, builtin, data FROM themes ORDER BY builtin DESC, name")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ThemeRow {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    builtin: r.get::<_, i64>(2)? != 0,
                    data: serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or(serde_json::json!({})),
                })
            })?
            .flatten()
            .collect();
        Ok(rows)
    }

    pub fn upsert_theme(&self, t: &ThemeRow) -> AppResult<()> {
        self.conn().execute(
            "INSERT INTO themes(id, name, builtin, data, created_at) VALUES(?1,?2,?3,?4,?5)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, data=excluded.data",
            params![t.id, t.name, t.builtin as i64, t.data.to_string(), now_ms()],
        )?;
        Ok(())
    }

    pub fn delete_theme(&self, id: &str) -> AppResult<usize> {
        Ok(self.conn().execute("DELETE FROM themes WHERE id = ?1 AND builtin = 0", params![id])?)
    }

    // ---- профили ----

    pub fn list_profiles(&self) -> AppResult<Vec<ProfileRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT id, name, builtin, data, updated_at FROM profiles ORDER BY created_at")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ProfileRow {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    builtin: r.get::<_, i64>(2)? != 0,
                    data: serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or(serde_json::json!({})),
                    updated_at: r.get(4)?,
                })
            })?
            .flatten()
            .collect();
        Ok(rows)
    }

    pub fn get_profile(&self, id: &str) -> AppResult<Option<ProfileRow>> {
        Ok(self
            .conn()
            .query_row("SELECT id, name, builtin, data, updated_at FROM profiles WHERE id = ?1", params![id], |r| {
                Ok(ProfileRow {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    builtin: r.get::<_, i64>(2)? != 0,
                    data: serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or(serde_json::json!({})),
                    updated_at: r.get(4)?,
                })
            })
            .optional()?)
    }

    pub fn upsert_profile(&self, p: &ProfileRow) -> AppResult<()> {
        let now = now_ms();
        self.conn().execute(
            "INSERT INTO profiles(id, name, builtin, data, created_at, updated_at) VALUES(?1,?2,?3,?4,?5,?5)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, data=excluded.data, updated_at=excluded.updated_at",
            params![p.id, p.name, p.builtin as i64, p.data.to_string(), now],
        )?;
        Ok(())
    }

    pub fn delete_profile(&self, id: &str) -> AppResult<usize> {
        Ok(self.conn().execute("DELETE FROM profiles WHERE id = ?1", params![id])?)
    }

    // ---- горячие клавиши ----

    pub fn list_hotkeys(&self) -> AppResult<Vec<HotkeyRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT action, accelerator, enabled FROM hotkeys")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(HotkeyRow { action: r.get(0)?, accelerator: r.get(1)?, enabled: r.get::<_, i64>(2)? != 0 })
            })?
            .flatten()
            .collect();
        Ok(rows)
    }

    pub fn upsert_hotkey(&self, h: &HotkeyRow) -> AppResult<()> {
        self.conn().execute(
            "INSERT INTO hotkeys(action, accelerator, enabled) VALUES(?1,?2,?3)
             ON CONFLICT(action) DO UPDATE SET accelerator=excluded.accelerator, enabled=excluded.enabled",
            params![h.action, h.accelerator, h.enabled as i64],
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetSetting {
    pub card_id: String,
    pub enabled: bool,
    pub sort_order: i64,
    pub priority: i64,
    #[serde(default)]
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeRow {
    pub id: String,
    pub name: String,
    pub builtin: bool,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileRow {
    pub id: String,
    pub name: String,
    pub builtin: bool,
    pub data: serde_json::Value,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyRow {
    pub action: String,
    pub accelerator: String,
    pub enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip() {
        let db = Database::open_memory();
        let mut s = db.load_settings().unwrap();
        s.appearance.radius = 14;
        s.clipboard.max_items = 42;
        db.save_all_settings(&s).unwrap();
        let loaded = db.load_settings().unwrap();
        assert_eq!(loaded.appearance.radius, 14);
        assert_eq!(loaded.clipboard.max_items, 42);
    }

    #[test]
    fn broken_section_falls_back() {
        let db = Database::open_memory();
        db.conn()
            .execute("INSERT INTO settings(key, value, updated_at) VALUES('appearance', '{not json', 0)", [])
            .unwrap();
        let s = db.load_settings().unwrap();
        assert_eq!(s.appearance.radius, 22);
    }
}
