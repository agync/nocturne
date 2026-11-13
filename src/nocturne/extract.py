"""Turn a captured item into durable facts."""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from typing import Protocol

from .lemonade import LemonadeClient
from .models import Event

SYSTEM = """You extract durable facts from one captured item.
Return JSON only:
{"facts": [{"subject": "...", "predicate": "...", "object": "...", "confidence": 0.0, "valid_from": null}]}
Rules:
- Only facts the item supports. No speculation.
- Use a small schema: person, project, decision, preference, event, place.
- Keep the subject and the object short.
- If the item has no durable fact, return {"facts": []}.
"""


@dataclass(slots=True)
class ExtractedFact:
    subject: str
    predicate: str
    obj: str
    confidence: float = 0.5
    valid_from: str | None = None


class Extractor(Protocol):
    def extract(self, event: Event) -> list[ExtractedFact]:
        """Return the durable facts in one event."""


class HeuristicExtractor:
    """Offline extractor for tests and first runs.

    It handles a small set of explicit patterns and never calls a model.
    """

    PATTERNS = (
        (re.compile(r"\bI prefer ([^.!?]+)", re.IGNORECASE), "user", "prefers"),
        (re.compile(r"\bI decided to ([^.!?]+)", re.IGNORECASE), "user", "decided"),
        (re.compile(r"\bI am working on ([^.!?]+)", re.IGNORECASE), "user", "works_on"),
        (re.compile(r"\bI live in ([^.!?]+)", re.IGNORECASE), "user", "lives_in"),
    )

    def extract(self, event: Event) -> list[ExtractedFact]:
        facts: list[ExtractedFact] = []
        for pattern, subject, predicate in self.PATTERNS:
            for match in pattern.finditer(event.content):
                facts.append(
                    ExtractedFact(
                        subject=subject,
                        predicate=predicate,
                        obj=match.group(1).strip(),
                        confidence=0.6,
                    )
                )
        return facts


class LemonadeExtractor:
    """Extractor backed by a local model through Lemonade."""

    def __init__(self, client: LemonadeClient, model: str):
        self.client = client
        self.model = model

    def extract(self, event: Event) -> list[ExtractedFact]:
        text = event.content[:12000]
        raw = self.client.chat(
            [
                {"role": "system", "content": SYSTEM},
                {
                    "role": "user",
                    "content": f"Kind: {event.kind}\nSource: {event.source}\n\n{text}",
                },
            ],
            model=self.model,
            temperature=0.0,
            max_tokens=1200,
        )
        return parse_facts(raw)


def parse_facts(raw: str) -> list[ExtractedFact]:
    """Parse a model response into facts, tolerating code fences."""
    text = raw.strip()
    if text.startswith("```"):
        text = re.sub(r"^```(?:json)?\s*", "", text)
        text = re.sub(r"\s*```$", "", text)
    try:
        payload = json.loads(text)
    except json.JSONDecodeError:
        match = re.search(r"\{.*\}", text, re.DOTALL)
        if not match:
            return []
        try:
            payload = json.loads(match.group(0))
        except json.JSONDecodeError:
            return []
    facts: list[ExtractedFact] = []
    for item in payload.get("facts", []):
        subject = str(item.get("subject", "")).strip()
        predicate = str(item.get("predicate", "")).strip()
        obj = str(item.get("object", "")).strip()
        if not subject or not predicate or not obj:
            continue
        try:
            confidence = float(item.get("confidence", 0.5))
        except (TypeError, ValueError):
            confidence = 0.5
        facts.append(
            ExtractedFact(
                subject=subject,
                predicate=predicate,
                obj=obj,
                confidence=max(0.0, min(1.0, confidence)),
                valid_from=item.get("valid_from"),
            )
        )
    return facts
