"""SQLite event tape and fact graph."""

from __future__ import annotations

import hashlib
import json
import re
import sqlite3
import uuid
from datetime import UTC, datetime
from pathlib import Path
from typing import Any, Self

from .models import Brief, Event, Fact, utcnow

SCHEMA = """
CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    source TEXT NOT NULL,
    content TEXT NOT NULL,
    captured_at TEXT NOT NULL,
    content_hash TEXT NOT NULL UNIQUE,
    metadata TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS facts (
    id TEXT PRIMARY KEY,
    subject TEXT NOT NULL,
    predicate TEXT NOT NULL,
    obj TEXT NOT NULL,
    valid_from TEXT,
    valid_to TEXT,
    recorded_at TEXT NOT NULL,
    confidence REAL NOT NULL,
    provenance TEXT NOT NULL DEFAULT '[]',
    sensitivity TEXT NOT NULL DEFAULT 'normal',
    status TEXT NOT NULL DEFAULT 'active',
    last_seen TEXT,
    ttl_days INTEGER
);

CREATE INDEX IF NOT EXISTS facts_subject_idx ON facts(subject);
CREATE INDEX IF NOT EXISTS facts_status_idx ON facts(status);
CREATE INDEX IF NOT EXISTS facts_last_seen_idx ON facts(last_seen);

CREATE VIRTUAL TABLE IF NOT EXISTS facts_fts USING fts5(
    subject, predicate, obj,
    content='facts',
    content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS facts_ai AFTER INSERT ON facts BEGIN
    INSERT INTO facts_fts(rowid, subject, predicate, obj)
    VALUES (new.rowid, new.subject, new.predicate, new.obj);
END;

CREATE TRIGGER IF NOT EXISTS facts_ad AFTER DELETE ON facts BEGIN
    INSERT INTO facts_fts(facts_fts, rowid, subject, predicate, obj)
    VALUES ('delete', old.rowid, old.subject, old.predicate, old.obj);
END;

CREATE TRIGGER IF NOT EXISTS facts_au AFTER UPDATE ON facts BEGIN
    INSERT INTO facts_fts(facts_fts, rowid, subject, predicate, obj)
    VALUES ('delete', old.rowid, old.subject, old.predicate, old.obj);
    INSERT INTO facts_fts(rowid, subject, predicate, obj)
    VALUES (new.rowid, new.subject, new.predicate, new.obj);
END;

CREATE TABLE IF NOT EXISTS briefs (
    id TEXT PRIMARY KEY,
    date TEXT NOT NULL,
    markdown TEXT NOT NULL,
    created_at TEXT NOT NULL,
    stats TEXT NOT NULL DEFAULT '{}'
);
"""


def parse_time(value: str) -> datetime:
    """Parse an ISO 8601 string and assume UTC when no zone is given."""
    parsed = datetime.fromisoformat(value)
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=UTC)
    return parsed


class Store:
    """The event tape and the fact graph live in one SQLite file."""

    def __init__(self, path: str | Path):
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.conn = sqlite3.connect(self.path)
        self.conn.row_factory = sqlite3.Row
        self.conn.execute("PRAGMA journal_mode=WAL")
        self.conn.execute("PRAGMA foreign_keys=ON")
        self.conn.executescript(SCHEMA)
        self.conn.commit()

    def close(self) -> None:
        self.conn.close()

    def __enter__(self) -> Self:
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()

    def add_event(
        self,
        kind: str,
        source: str,
        content: str,
        captured_at: str | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> Event:
        digest = hashlib.sha256(content.encode("utf-8")).hexdigest()
        row = self.conn.execute(
            "SELECT * FROM events WHERE content_hash = ?", (digest,)
        ).fetchone()
        if row:
            return self._event(row)
        event_id = f"evt_{uuid.uuid4().hex[:16]}"
        captured = captured_at or utcnow()
        meta = json.dumps(metadata or {}, ensure_ascii=False, sort_keys=True)
        self.conn.execute(
            "INSERT INTO events (id, kind, source, content, captured_at, content_hash, metadata) "
            "VALUES (?, ?, ?, ?, ?, ?, ?)",
            (event_id, kind, source, content, captured, digest, meta),
        )
        self.conn.commit()
        return Event(event_id, kind, source, content, captured, digest, metadata or {})

    def get_event(self, event_id: str) -> Event | None:
        row = self.conn.execute("SELECT * FROM events WHERE id = ?", (event_id,)).fetchone()
        return self._event(row) if row else None

    def recent_events(self, since: str | None = None, limit: int = 1000) -> list[Event]:
        sql = "SELECT * FROM events"
        params: list[Any] = []
        if since:
            sql += " WHERE captured_at >= ?"
            params.append(since)
        sql += " ORDER BY captured_at DESC LIMIT ?"
        params.append(limit)
        return [self._event(row) for row in self.conn.execute(sql, params)]

    def add_fact(
        self,
        subject: str,
        predicate: str,
        obj: str,
        confidence: float = 0.5,
        provenance: list[str] | None = None,
        valid_from: str | None = None,
        valid_to: str | None = None,
        sensitivity: str = "normal",
        recorded_at: str | None = None,
        last_seen: str | None = None,
        ttl_days: int | None = None,
    ) -> Fact:
        subject = subject.strip()
        predicate = predicate.strip()
        obj = obj.strip()
        if not subject or not predicate or not obj:
            raise ValueError("subject, predicate, and object are required")
        now = recorded_at or utcnow()
        seen = last_seen or now
        row = self.conn.execute(
            "SELECT * FROM facts WHERE subject = ? AND predicate = ? AND obj = ? "
            "AND status != 'deleted' ORDER BY recorded_at DESC LIMIT 1",
            (subject, predicate, obj),
        ).fetchone()
        if row:
            fact = self._fact(row)
            merged = sorted(set(fact.provenance) | set(provenance or []))
            new_confidence = min(1.0, max(fact.confidence, confidence) + 0.05)
            self.conn.execute(
                "UPDATE facts SET confidence = ?, last_seen = ?, provenance = ?, "
                "status = CASE WHEN status = 'archived' THEN 'active' ELSE status END "
                "WHERE id = ?",
                (new_confidence, seen, json.dumps(merged), fact.id),
            )
            self.conn.commit()
            return self.get_fact(fact.id)  # type: ignore[return-value]
        fact_id = f"fact_{uuid.uuid4().hex[:16]}"
        self.conn.execute(
            "INSERT INTO facts (id, subject, predicate, obj, valid_from, valid_to, recorded_at, "
            "confidence, provenance, sensitivity, status, last_seen, ttl_days) "
            "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', ?, ?)",
            (
                fact_id,
                subject,
                predicate,
                obj,
                valid_from,
                valid_to,
                now,
                max(0.0, min(1.0, confidence)),
                json.dumps(sorted(set(provenance or []))),
                sensitivity,
                seen,
                ttl_days,
            ),
        )
        self.conn.commit()
        return self.get_fact(fact_id)  # type: ignore[return-value]

    def get_fact(self, fact_id: str) -> Fact | None:
        row = self.conn.execute("SELECT * FROM facts WHERE id = ?", (fact_id,)).fetchone()
        return self._fact(row) if row else None

    def search(self, query: str, limit: int = 10) -> list[Fact]:
        tokens = [token for token in re.findall(r"[A-Za-z0-9_]+", query) if token]
        if not tokens:
            like = f"%{query}%"
            rows = self.conn.execute(
                "SELECT * FROM facts WHERE status != 'deleted' AND "
                "(subject LIKE ? OR predicate LIKE ? OR obj LIKE ?) "
                "ORDER BY last_seen DESC LIMIT ?",
                (like, like, like, limit),
            ).fetchall()
            return [self._fact(row) for row in rows]
        fts = " OR ".join(f'"{token}"' for token in tokens)
        rows = self.conn.execute(
            "SELECT f.* FROM facts f JOIN facts_fts ON facts_fts.rowid = f.rowid "
            "WHERE facts_fts MATCH ? AND f.status != 'deleted' "
            "ORDER BY bm25(facts_fts) LIMIT ?",
            (fts, limit),
        ).fetchall()
        return [self._fact(row) for row in rows]

    def facts_about(self, subject: str, limit: int = 50) -> list[Fact]:
        rows = self.conn.execute(
            "SELECT * FROM facts WHERE subject LIKE ? AND status != 'deleted' "
            "ORDER BY COALESCE(last_seen, recorded_at) DESC LIMIT ?",
            (f"%{subject}%", limit),
        ).fetchall()
        return [self._fact(row) for row in rows]

    def list_facts(self, status: str | None = None, limit: int = 50) -> list[Fact]:
        sql = "SELECT * FROM facts WHERE status != 'deleted'"
        params: list[Any] = []
        if status:
            sql += " AND status = ?"
            params.append(status)
        sql += " ORDER BY COALESCE(last_seen, recorded_at) DESC LIMIT ?"
        params.append(limit)
        return [self._fact(row) for row in self.conn.execute(sql, params)]

    def decay(
        self,
        now: str | None = None,
        fade_days: int = 30,
        archive_days: int = 90,
    ) -> int:
        current = parse_time(now) if now else datetime.now(UTC)
        changed = 0
        rows = self.conn.execute(
            "SELECT * FROM facts WHERE status IN ('active', 'fading')"
        ).fetchall()
        for row in rows:
            fact = self._fact(row)
            last = parse_time(fact.last_seen or fact.recorded_at)
            age = (current - last).days
            status = fact.status
            confidence = fact.confidence
            if age >= archive_days:
                status = "archived"
                confidence = min(confidence, 0.1)
            elif age >= fade_days:
                status = "fading"
                confidence = max(0.1, confidence * 0.9)
            if status != fact.status or abs(confidence - fact.confidence) > 1e-9:
                self.conn.execute(
                    "UPDATE facts SET status = ?, confidence = ? WHERE id = ?",
                    (status, confidence, fact.id),
                )
                changed += 1
        self.conn.commit()
        return changed

    def forget(self, fact_id: str) -> bool:
        cursor = self.conn.execute("DELETE FROM facts WHERE id = ?", (fact_id,))
        self.conn.commit()
        return cursor.rowcount > 0

    def archive(self, fact_id: str) -> bool:
        cursor = self.conn.execute(
            "UPDATE facts SET status = 'archived' WHERE id = ?", (fact_id,)
        )
        self.conn.commit()
        return cursor.rowcount > 0

    def restore(self, fact_id: str) -> bool:
        cursor = self.conn.execute(
            "UPDATE facts SET status = 'active', last_seen = ? WHERE id = ?",
            (utcnow(), fact_id),
        )
        self.conn.commit()
        return cursor.rowcount > 0

    def stats(self) -> dict[str, Any]:
        counts = {
            row["status"]: row["n"]
            for row in self.conn.execute(
                "SELECT status, COUNT(*) AS n FROM facts GROUP BY status"
            )
        }
        events = self.conn.execute("SELECT COUNT(*) AS n FROM events").fetchone()["n"]
        briefs = self.conn.execute("SELECT COUNT(*) AS n FROM briefs").fetchone()["n"]
        result: dict[str, Any] = {
            "events": events,
            "facts": sum(counts.values()),
            "briefs": briefs,
        }
        for status, count in counts.items():
            result[f"facts_{status}"] = count
        return result

    def save_brief(
        self, date: str, markdown: str, stats: dict[str, Any] | None = None
    ) -> Brief:
        brief_id = f"brief_{uuid.uuid4().hex[:16]}"
        created = utcnow()
        payload = json.dumps(stats or {}, ensure_ascii=False, sort_keys=True)
        self.conn.execute(
            "INSERT INTO briefs (id, date, markdown, created_at, stats) "
            "VALUES (?, ?, ?, ?, ?)",
            (brief_id, date, markdown, created, payload),
        )
        self.conn.commit()
        return Brief(brief_id, date, markdown, created, stats or {})

    def latest_brief(self) -> Brief | None:
        row = self.conn.execute(
            "SELECT * FROM briefs ORDER BY created_at DESC LIMIT 1"
        ).fetchone()
        return self._brief(row) if row else None

    def _event(self, row: sqlite3.Row) -> Event:
        return Event(
            id=row["id"],
            kind=row["kind"],
            source=row["source"],
            content=row["content"],
            captured_at=row["captured_at"],
            content_hash=row["content_hash"],
            metadata=json.loads(row["metadata"]),
        )

    def _fact(self, row: sqlite3.Row) -> Fact:
        return Fact(
            id=row["id"],
            subject=row["subject"],
            predicate=row["predicate"],
            obj=row["obj"],
            recorded_at=row["recorded_at"],
            confidence=row["confidence"],
            valid_from=row["valid_from"],
            valid_to=row["valid_to"],
            provenance=json.loads(row["provenance"]),
            sensitivity=row["sensitivity"],
            status=row["status"],
            last_seen=row["last_seen"],
            ttl_days=row["ttl_days"],
        )

    def _brief(self, row: sqlite3.Row) -> Brief:
        return Brief(
            id=row["id"],
            date=row["date"],
            markdown=row["markdown"],
            created_at=row["created_at"],
            stats=json.loads(row["stats"]),
        )
