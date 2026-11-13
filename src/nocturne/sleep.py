"""The nightly consolidation cycle."""

from __future__ import annotations

from datetime import date as date_cls

from .extract import Extractor
from .models import Brief
from .paper import Paper
from .store import Store


class SleepCycle:
    """Turn one day of events into facts, a decay pass, and a paper."""

    def __init__(self, store: Store, extractor: Extractor, paper: Paper):
        self.store = store
        self.extractor = extractor
        self.paper = paper

    def run(
        self,
        day: date_cls | None = None,
        fade_days: int = 30,
        archive_days: int = 90,
    ) -> Brief:
        target = day or date_cls.today()
        since = f"{target.isoformat()}T00:00:00+00:00"
        events = self.store.recent_events(since=since, limit=5000)

        extracted = 0
        for event in events:
            for fact in self.extractor.extract(event):
                self.store.add_fact(
                    subject=fact.subject,
                    predicate=fact.predicate,
                    obj=fact.obj,
                    confidence=fact.confidence,
                    provenance=[event.id],
                    valid_from=fact.valid_from,
                )
                extracted += 1

        decayed = self.store.decay(
            fade_days=fade_days, archive_days=archive_days
        )
        stats = self.store.stats()
        stats.update(
            {
                "events_today": len(events),
                "extracted": extracted,
                "decayed": decayed,
            }
        )
        markdown = self.paper.render(target.isoformat(), stats)
        return self.store.save_brief(target.isoformat(), markdown, stats)
