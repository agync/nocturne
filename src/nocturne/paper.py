"""The morning paper: what changed, what needs you, what it forgot."""

from __future__ import annotations

import re

from .lemonade import LemonadeClient
from .models import Fact
from .store import Store


def _line(fact: Fact) -> str:
    return (
        f"- {fact.subject} {fact.predicate.replace('_', ' ')} {fact.obj} "
        f"(confidence {fact.confidence:.2f}) [{fact.id}]"
    )


def plain_text(markdown: str) -> str:
    """Strip the Markdown markers a speech model should not read aloud."""
    text = re.sub(r"\[fact_[^\]]+\]", "", markdown)
    text = re.sub(r"^#+\s*", "", text, flags=re.MULTILINE)
    text = re.sub(r"^[-*]\s*", "", text, flags=re.MULTILINE)
    text = text.replace("_", "")
    return re.sub(r"\n{3,}", "\n\n", text).strip()


class Paper:
    def __init__(self, store: Store, client: LemonadeClient | None = None):
        self.store = store
        self.client = client

    def render(self, day: str, stats: dict[str, object]) -> str:
        active = self.store.list_facts(status="active", limit=8)
        fading = self.store.list_facts(status="fading", limit=6)
        archived = self.store.list_facts(status="archived", limit=6)

        lines = [
            f"# The Morning Paper: {day}",
            "",
            (
                f"_{stats.get('events_today', 0)} events, "
                f"{stats.get('facts', 0)} facts, "
                f"{stats.get('facts_fading', 0)} fading, "
                f"{stats.get('facts_archived', 0)} forgotten._"
            ),
            "",
            "## What changed",
            "",
        ]
        if active:
            lines.extend(_line(fact) for fact in active)
        else:
            lines.append("Nothing yet. Add something with `nocturne remember`.")

        lines.extend(["", "## Needs you", ""])
        if fading:
            lines.extend(_line(fact) for fact in fading)
        else:
            lines.append("Nothing is fading today.")

        lines.extend(["", "## Forgotten", ""])
        if archived:
            lines.extend(_line(fact) for fact in archived)
        else:
            lines.append("Nothing was archived today.")

        lines.extend(["", "## One question", ""])
        if fading:
            fact = fading[0]
            lines.append(
                f"You have not used `{fact.subject} {fact.predicate} {fact.obj}` in a while. "
                "Keep it, or let it fade?"
            )
        elif active:
            fact = active[0]
            lines.append(
                f"Is `{fact.subject} {fact.predicate} {fact.obj}` still true?"
            )
        else:
            lines.append("What is one thing worth remembering from today?")

        return "\n".join(lines) + "\n"

    def render_audio(self, markdown: str, voice: str = "af_heart") -> bytes:
        if self.client is None:
            raise RuntimeError("a Lemonade client is required for audio")
        return self.client.speak(plain_text(markdown), voice=voice)
