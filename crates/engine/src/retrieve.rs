//! Retrieval: full-text and vector search fused by reciprocal rank, then ordered by status.
//!
//! Active, fading and unconfirmed memories come first; let-go memories only fill what is left,
//! flagged. Unconfirmed hits carry `status: unconfirmed` so the answer layer never states them
//! as true. Only current memories are searched. Searching does not count as use.
//!
//! A question about who someone is ("Who is Leo?") brings the memories that say how that person
//! is related to the person asking to the front, whatever the ranking found.

use std::collections::HashMap;
use std::sync::Arc;

use nocturne_contract::MemoryStatus;
use rusqlite::Connection;

use crate::Engine;
use crate::error::Result;
use crate::rows::{load_mem, mem_view, query_mems};
use crate::types::{Hit, MemoryId};
use crate::util::{fts_or, key_of};

/// Reciprocal rank fusion constant.
const RRF_K: f32 = 60.0;
/// Candidates taken from each ranking before fusion.
const POOL: usize = 60;
/// Vectors less similar than this to the query are not hits at all.
const MIN_COSINE: f32 = 0.25;
/// Predicates that say how someone is related to the person or to someone else: `knows`, and
/// the words a mind writes when it reaches for one of its own.
const RELATION_PREDICATES: [&str; 5] = ["knows", "relationship", "relation", "related_to", "is"];
/// How a question about who someone is can start.
const WHO_IS: [&str; 4] = ["who is ", "who's ", "who was ", "who are "];

/// The name in a question about who someone is, as a subject key: "Who is Leo?" gives "leo".
fn who_is(query: &str) -> Option<String> {
    let q = query.trim().to_lowercase();
    let rest = WHO_IS.iter().find_map(|p| q.strip_prefix(p))?;
    let name = key_of(rest.trim_end_matches(['?', '.', '!', ' ']));
    (!name.is_empty() && name != "me").then_some(name)
}

/// All current vectors, normalised, held in memory so a search is one pass over them.
pub(crate) struct VecIndex {
    pub dim: usize,
    pub ids: Vec<String>,
    pub data: Vec<f32>,
}

fn normalise(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
}

fn build_index(conn: &Connection) -> Result<VecIndex> {
    let mut stmt = conn.prepare(
        "SELECT e.memory_id, e.dim, e.vec FROM embeddings e \
         JOIN memories m ON m.id = e.memory_id WHERE m.superseded_at IS NULL",
    )?;
    let mut index = VecIndex {
        dim: 0,
        ids: Vec::new(),
        data: Vec::new(),
    };
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let dim: usize = row.get::<_, i64>(1)? as usize;
        let blob: Vec<u8> = row.get(2)?;
        if dim == 0 || blob.len() != dim * 4 {
            continue;
        }
        if index.dim == 0 {
            index.dim = dim;
        }
        if dim != index.dim {
            continue;
        }
        let mut v: Vec<f32> = blob
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        normalise(&mut v);
        index.ids.push(id);
        index.data.extend_from_slice(&v);
    }
    Ok(index)
}

impl Engine {
    fn vector_index(&self) -> Result<Arc<VecIndex>> {
        if let Some(i) = self
            .inner
            .vectors
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            return Ok(i.clone());
        }
        let built = Arc::new(self.read(build_index)?);
        *self
            .inner
            .vectors
            .write()
            .unwrap_or_else(|e| e.into_inner()) = Some(built.clone());
        Ok(built)
    }

    /// Find memories for a question: words through full-text search, and, when the caller has a
    /// vector for the query, meaning through cosine similarity. Returns at most `k` hits.
    pub fn retrieve(&self, query: &str, query_vec: Option<&[f32]>, k: usize) -> Result<Vec<Hit>> {
        if k == 0 {
            return Ok(Vec::new());
        }
        let mut score: HashMap<String, f32> = HashMap::new();

        if let Some(q) = fts_or(query) {
            let ids: Vec<String> = self.read(|conn| {
                let mut stmt = conn.prepare_cached(
                    "SELECT m.id FROM memories_fts f JOIN memories m ON m.rowid = f.rowid \
                     WHERE memories_fts MATCH ?1 AND m.superseded_at IS NULL \
                     ORDER BY f.rank LIMIT ?2",
                )?;
                let rows = stmt.query_map(rusqlite::params![q, POOL as i64], |r| r.get(0))?;
                Ok(rows.collect::<rusqlite::Result<_>>()?)
            })?;
            for (rank, id) in ids.into_iter().enumerate() {
                *score.entry(id).or_default() += 1.0 / (RRF_K + rank as f32 + 1.0);
            }
        }

        if let Some(qv) = query_vec.filter(|v| !v.is_empty()) {
            let index = self.vector_index()?;
            if index.dim == qv.len() {
                let mut q = qv.to_vec();
                normalise(&mut q);
                let mut sims: Vec<(f32, usize)> = index
                    .data
                    .chunks_exact(index.dim)
                    .enumerate()
                    .map(|(i, v)| (v.iter().zip(&q).map(|(a, b)| a * b).sum::<f32>(), i))
                    .filter(|(s, _)| *s >= MIN_COSINE)
                    .collect();
                sims.sort_by(|a, b| b.0.total_cmp(&a.0));
                for (rank, (_, i)) in sims.into_iter().take(POOL).enumerate() {
                    *score.entry(index.ids[i].clone()).or_default() +=
                        1.0 / (RRF_K + rank as f32 + 1.0);
                }
            }
        }

        let mut ranked: Vec<(String, f32)> = score.into_iter().collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        self.read(|conn| {
            let mut live: Vec<Hit> = Vec::new();
            let mut let_go: Vec<Hit> = Vec::new();
            for (id, s) in ranked {
                // The vector index can lag a correction by one call; the row is the authority.
                let Some(row) = load_mem(conn, &id)? else {
                    continue;
                };
                if row.superseded_at.is_some() {
                    continue;
                }
                let is_let_go = row.status == MemoryStatus::LetGo;
                let hit = Hit {
                    memory: mem_view(conn, &row)?,
                    score: s,
                    let_go: is_let_go,
                };
                if is_let_go {
                    let_go.push(hit);
                } else {
                    live.push(hit);
                }
                if live.len() >= k {
                    break;
                }
            }
            if let Some(name) = who_is(query) {
                let marks = vec!["?"; RELATION_PREDICATES.len()].join(", ");
                let mut params: Vec<&str> = vec![&name];
                params.extend(RELATION_PREDICATES);
                let related = query_mems(
                    conn,
                    &format!(
                        "m.subject_key = ?1 AND m.predicate IN ({marks}) \
                         AND m.superseded_at IS NULL AND m.forgetting = 0 \
                         AND m.status != 'letGo' ORDER BY m.recorded_at DESC, m.id"
                    ),
                    rusqlite::params_from_iter(params),
                )?;
                for (at, row) in related.iter().enumerate() {
                    let found = live.iter().position(|h| h.memory.id == row.id);
                    let hit = match found {
                        Some(i) => live.remove(i),
                        None => Hit {
                            memory: mem_view(conn, row)?,
                            score: 0.0,
                            let_go: false,
                        },
                    };
                    live.insert(at, hit);
                }
            }
            live.truncate(k);
            let room = k - live.len();
            live.extend(let_go.into_iter().take(room));
            Ok(live)
        })
    }

    /// The open which-is-right questions that involve any of `ids`, each as the two memories it
    /// is between.
    pub fn open_disputes(&self, ids: &[MemoryId]) -> Result<Vec<(MemoryId, MemoryId)>> {
        self.read(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT a, b FROM conflicts WHERE resolved_at IS NULL AND (a = ?1 OR b = ?1) \
                 ORDER BY created_at, id",
            )?;
            let mut out: Vec<(MemoryId, MemoryId)> = Vec::new();
            for id in ids {
                let rows = stmt.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?;
                for pair in rows {
                    let pair = pair?;
                    if !out.contains(&pair) {
                        out.push(pair);
                    }
                }
            }
            Ok(out)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::who_is;

    #[test]
    fn a_question_about_who_someone_is_names_them() {
        assert_eq!(who_is("Who is Leo?").as_deref(), Some("leo"));
        assert_eq!(who_is("  who's Mrs Carver ").as_deref(), Some("mrs carver"));
        assert_eq!(who_is("Who was the plumber?").as_deref(), Some("plumber"));
        assert_eq!(who_is("When is Leo's trip?"), None);
        assert_eq!(who_is("Who is?"), None);
    }
}
