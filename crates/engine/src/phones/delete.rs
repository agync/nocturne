//! "Delete what came in": a phone's calendar entries or readings, with their earlier versions
//! and what Nocturne learned from them alone, through the same preview and token as every
//! forget. A memory the person's own keeps also say stays, with only the phone's receipt taken
//! off.

use std::collections::BTreeSet;

use nocturne_contract::{PhoneDeletePreview, PhoneDeleteScope, PhoneSource};
use rusqlite::{Connection, OptionalExtension, params};

use super::source_str;
use crate::Engine;
use crate::error::{Error, Result};
use crate::types::ForgetOutcome;
use crate::util::enum_str;

/// The current versions a scope covers.
fn items_in(conn: &Connection, scope: &PhoneDeleteScope) -> Result<Vec<String>> {
    if let Some(id) = &scope.item_id {
        let ours: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM phone_records \
             WHERE item_id = ?1 AND device_id = ?2 AND source = ?3)",
            params![id, scope.device_id, source_str(scope.source)],
            |r| r.get(0),
        )?;
        return Ok(if ours { vec![id.clone()] } else { Vec::new() });
    }
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

/// What the items and their earlier versions said: the memories only they say, the ones
/// something else says too, and how many earlier versions there are.
struct Said {
    only: Vec<String>,
    shared: Vec<String>,
    versions: u32,
}

fn said(conn: &Connection, items: &[String]) -> Result<Said> {
    let mut doomed: BTreeSet<String> = BTreeSet::new();
    let mut versions = 0u32;
    for item in items {
        let earlier = crate::journal::keep::earlier_versions(conn, item)?;
        versions += earlier.len() as u32;
        doomed.insert(item.clone());
        doomed.extend(earlier);
    }
    let mut memories: BTreeSet<String> = BTreeSet::new();
    for i in &doomed {
        let mut stmt =
            conn.prepare_cached("SELECT memory_id FROM memory_sources WHERE item_id = ?1")?;
        for m in stmt.query_map([i], |r| r.get::<_, String>(0))? {
            memories.insert(m?);
        }
    }
    let doomed_json = serde_json::to_string(&doomed)?;
    let (mut only, mut shared) = (Vec::new(), Vec::new());
    for m in memories {
        let elsewhere: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM memory_sources WHERE memory_id = ?1 \
             AND item_id NOT IN (SELECT value FROM json_each(?2)))",
            params![m, doomed_json],
            |r| r.get(0),
        )?;
        if elsewhere {
            shared.push(m);
        } else {
            only.push(m);
        }
    }
    Ok(Said {
        only,
        shared,
        versions,
    })
}

impl Engine {
    /// What deleting a scope would remove, and the token that applies it.
    pub fn preview_phone_delete(&self, scope: PhoneDeleteScope) -> Result<PhoneDeletePreview> {
        let (items, said) = self.read(|conn| {
            let items = items_in(conn, &scope)?;
            let said = said(conn, &items)?;
            Ok((items, said))
        })?;
        if items.is_empty() {
            return Err(Error::not_found("nothing came in from that phone"));
        }
        let preview = self.preview_delete_with(&said.only, true, &items, true, &said.shared)?;
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
        let originals = self.originals_dir();
        let noticing = self.read(|conn| {
            if scope.source != PhoneSource::Health {
                return Ok(0);
            }
            let ctx = crate::journal::Ctx::load(conn, self.clock())?;
            crate::journal::lookback::noticing_changed_by(conn, &ctx, &originals, &items)
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
            versions: said.versions,
            memories: preview.memories.len() as u32,
            page_lines,
            recap_lines: preview.reflection_lines.unwrap_or(0),
            noticing,
            shared_memories: said.shared.len() as u32,
        })
    }

    /// Apply a previewed delete. With `also_stop`, the phone's source is stopped too, so what
    /// was deleted does not come back with its next send. A delete of the whole calendar also
    /// forgets the names of the phone's calendars.
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
        if also_stop {
            // Stopped first, so a send arriving while the delete runs is not taken.
            self.set_source_taking(&scope.device_id, scope.source, false)?;
        }
        let outcome = self.forget(token)?;
        let whole = scope.kinds.is_none() && scope.before.is_none() && scope.item_id.is_none();
        if whole && scope.source == PhoneSource::Calendar {
            self.write(|tx| {
                tx.execute(
                    "UPDATE phone_sources SET calendars = '[]' \
                     WHERE device_id = ?1 AND source = 'calendar'",
                    [&scope.device_id],
                )?;
                Ok(())
            })?;
        }
        Ok(outcome)
    }

    /// Everything a phone brought, from both sources, with its sources stopped first: for
    /// removing a phone with "Also delete what it brought". Once the phone is removed,
    /// [`Engine::forget_phone_sources`] takes what is kept about its sources.
    pub fn delete_brought(&self, device_id: &str) -> Result<()> {
        for source in [PhoneSource::Calendar, PhoneSource::Health] {
            self.set_source_taking(device_id, source, false)?;
        }
        for source in [PhoneSource::Calendar, PhoneSource::Health] {
            let scope = PhoneDeleteScope {
                device_id: device_id.to_string(),
                source,
                kinds: None,
                before: None,
                item_id: None,
            };
            match self.preview_phone_delete(scope) {
                Ok(p) => {
                    self.forget(&p.token)?;
                }
                Err(Error::NotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }
        self.forget_phone_sources(device_id)
    }

    /// What is kept about a removed phone's sources (its calendars' names, when it last sent),
    /// once nothing it brought is left. Nothing happens while it is paired: its sources stay
    /// stopped, so a send in between is refused.
    pub fn forget_phone_sources(&self, device_id: &str) -> Result<()> {
        self.write(|tx| {
            tx.execute(
                "DELETE FROM phone_sources WHERE device_id = ?1 \
                 AND EXISTS (SELECT 1 FROM remote_devices WHERE id = ?1 AND revoked_at IS NOT NULL) \
                 AND NOT EXISTS (SELECT 1 FROM phone_records WHERE device_id = ?1)",
                [device_id],
            )?;
            Ok(())
        })
    }
}
