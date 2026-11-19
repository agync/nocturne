//! Mentions: which person, place or thing an item names, and where.

use std::collections::HashMap;

use nocturne_contract::{EntityKind, Mention, MentionState};
use rusqlite::{Connection, params_from_iter};

use crate::error::Result;
use crate::util::parse_enum;

/// The mentions of each of `item_ids`, in the order they appear in the item. A mention the person
/// removed is returned with its state, so the interface can offer Undo; callers that show only
/// live names filter on `state`.
pub(crate) fn mentions_for_items(
    conn: &Connection,
    item_ids: &[String],
) -> Result<HashMap<String, Vec<Mention>>> {
    let mut out: HashMap<String, Vec<Mention>> = HashMap::new();
    if item_ids.is_empty() {
        return Ok(out);
    }
    let marks = vec!["?"; item_ids.len()].join(", ");
    let sql = format!(
        "SELECT n.id, n.item_id, n.entity_id, e.name, e.kind, e.role, n.start, n.end, n.state, \
         n.new, (SELECT COUNT(*) FROM memory_sources s JOIN memories m ON m.id = s.memory_id \
                 WHERE s.item_id = n.item_id AND m.entity_id = n.entity_id \
                 AND m.superseded_at IS NULL AND m.forgetting = 0) \
         FROM mentions n JOIN entities e ON e.id = n.entity_id \
         WHERE n.item_id IN ({marks}) ORDER BY n.item_id, n.ord"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(item_ids.iter()), |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, Option<u32>>(6)?,
            r.get::<_, Option<u32>>(7)?,
            r.get::<_, String>(8)?,
            r.get::<_, bool>(9)?,
            r.get::<_, u32>(10)?,
        ))
    })?;
    for row in rows {
        let (id, item_id, entity_id, name, kind, role, start, end, state, new, memory_count) = row?;
        out.entry(item_id.clone()).or_default().push(Mention {
            id,
            item_id,
            entity_id,
            name,
            kind: parse_enum::<EntityKind>(&kind)?,
            role,
            start,
            end,
            state: parse_enum::<MentionState>(&state)?,
            new,
            memory_count,
        });
    }
    Ok(out)
}
