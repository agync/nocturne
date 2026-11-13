"""Data types for the event tape and the fact graph."""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import UTC, datetime
from typing import Any


def utcnow() -> str:
    """Return the current UTC time as an ISO 8601 string."""
    return datetime.now(UTC).isoformat(timespec="seconds")


@dataclass(slots=True)
class Event:
    """One captured item, stored exactly as it arrived."""

    id: str
    kind: str
    source: str
    content: str
    captured_at: str
    content_hash: str
    metadata: dict[str, Any] = field(default_factory=dict)


@dataclass(slots=True)
class Fact:
    """One durable claim, with provenance and a validity interval."""

    id: str
    subject: str
    predicate: str
    obj: str
    recorded_at: str
    confidence: float = 0.5
    valid_from: str | None = None
    valid_to: str | None = None
    provenance: list[str] = field(default_factory=list)
    sensitivity: str = "normal"
    status: str = "active"
    last_seen: str | None = None
    ttl_days: int | None = None


@dataclass(slots=True)
class Brief:
    """One generated morning paper."""

    id: str
    date: str
    markdown: str
    created_at: str
    stats: dict[str, Any] = field(default_factory=dict)
