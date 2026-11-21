//! "Delete what came in": a phone's calendar entries or readings, with their earlier versions
//! and what Nocturne learned from them, through the same preview and token as every forget.

use nocturne_contract::{PhoneDeletePreview, PhoneDeleteScope, PhoneSource};
use rusqlite::{Connection, OptionalExtension, params};

use super::source_str;
use crate::Engine;
use crate::error::{Error, Result};
use crate::types::ForgetOutcome;
use crate::util::enum_str;

/// The current versions a scope covers.
fn items_in(conn: &Connection, scope: &PhoneDeleteScope) -> Result<Vec<String>> {
    if let Some(b) = &scope.before
        && b.parse::<jiff::civil::Date>().is_err()
    {
        return Err(Error::invalid("before is a day"));
    }
    let kinds: Option<Vec<String>> = match (&scope.kinds, scope.source) {
        (Some(k), PhoneSource::Health) => Some(k.iter().map(enum_str).collect()),
        _ => None,
    };
    let mut stmt = conn.prepare(
        "SELECT r.item_id, json_extract(i.metadata, '$.reading.type') \
         FROM phone_records r JOIN items i ON i.id = r.item_id \
         WHERE r.device_id = ?1 AND r.source = ?2 \
         AND (?3 IS NULL OR json_extract(i.metadata, '$.day') < ?3) \
         ORDER BY i.kept_at, i.id",
    )?;
    let rows: Vec<(String, Option<String>)> = stmt
        .query_map(
            params![scope.device_id, source_str(scope.source), scope.before],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows
        .into_iter()
        .filter(|(_, k)| match (&kinds, k) {
            (Some(ks), Some(k)) => ks.contains(k),
            (Some(_), None) => false,
            (None, _) => true,
        })
        .map(|(id, _)| id)
        .collect())
}

/// Every memory these items or their earlier versions said.
fn memories_of(conn: &Connection, items: &[String]) -> Result<(Vec<String>, u32)> {
    let mut memories = std::collections::BTreeSet::new();
    let mut versions = 0u32;
    for item in items {
        let mut all = vec![item.clone()];
        let earlier = crate::journal::keep::earlier_versions(conn, item)?;
        versions += earlier.len() as u32;
        all.extend(earlier);
        for i in all {
            let mut stmt =
                conn.prepare_cached("SELECT memory_id FROM memory_sources WHERE item_id = ?1")?;
            for m in stmt.query_map([&i], |r| r.get::<_, String>(0))? {
                memories.insert(m?);
            }
        }
    }
    Ok((memories.into_iter().collect(), versions))
}

impl Engine {
    /// What deleting a scope would remove, and the token that applies it.
    pub fn preview_phone_delete(&self, scope: PhoneDeleteScope) -> Result<PhoneDeletePreview> {
        let (items, memories, versions) = self.read(|conn| {
            let items = items_in(conn, &scope)?;
            let (memories, versions) = memories_of(conn, &items)?;
            Ok((items, memories, versions))
        })?;
        if items.is_empty() {
            return Err(Error::not_found("nothing came in from that phone"));
        }
        let preview = self.preview_delete(&memories, true, &items, true)?;
        let page_lines: u32 = self.read(|conn| {
            let mut n = 0u32;
            for m in &preview.memories {
                n += conn.query_row(
                    "SELECT COUNT(*) FROM page_line_memories l JOIN page_lines p ON p.id = l.line_id \
                     WHERE l.memory_id = ?1 AND p.forgotten = 0",
                    [&m.memory_id],
                    |r| r.get::<_, u32>(0),
                )?;
            }
            Ok(n)
        })?;
        self.write(|tx| {
            tx.execute(
                "UPDATE forget_tokens SET phone_scope = ?2 WHERE token = ?1",
                params![preview.token, serde_json::to_string(&scope)?],
            )?;
            Ok(())
        })?;
        Ok(PhoneDeletePreview {
            token: preview.token,
            items: items.len() as u32,
            versions,
            memories: preview.memories.len() as u32,
            page_lines,
            recap_lines: preview.reflection_lines.unwrap_or(0),
            // Noticing is worked out in recaps from what remains; nothing is stored for it yet.
            noticing: 0,
        })
    }

    /// Apply a previewed delete. With `also_stop`, the phone's source is stopped too, so what
    /// was deleted does not come back with its next send.
    pub fn delete_phone_data(&self, token: &str, also_stop: bool) -> Result<ForgetOutcome> {
        let scope: PhoneDeleteScope = self
            .read(|conn| {
                Ok(conn
                    .query_row(
                        "SELECT phone_scope FROM forget_tokens WHERE token = ?1",
                        [token],
                        |r| r.get::<_, Option<String>>(0),
                    )
                    .optional()?
                    .flatten())
            })?
            .and_then(|s| serde_json::from_str(&s).ok())
            .ok_or(Error::StaleToken)?;
        let outcome = self.forget(token)?;
        if also_stop {
            self.set_source_taking(&scope.device_id, scope.source, false)?;
        }
        Ok(outcome)
    }

    /// Everything a phone brought, from both sources: for removing a phone with "Also delete
    /// what it brought".
    pub fn delete_brought(&self, device_id: &str) -> Result<()> {
        for source in [PhoneSource::Calendar, PhoneSource::Health] {
            let scope = PhoneDeleteScope {
                device_id: device_id.to_string(),
                source,
                kinds: None,
                before: None,
            };
            match self.preview_phone_delete(scope) {
                Ok(p) => {
                    self.forget(&p.token)?;
                }
                Err(Error::NotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}
