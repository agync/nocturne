from datetime import date

from nocturne.extract import HeuristicExtractor
from nocturne.mcp import handle
from nocturne.paper import Paper
from nocturne.sleep import SleepCycle
from nocturne.store import Store


def make_store(tmp_path):
    return Store(tmp_path / "nocturne.sqlite3")


def test_event_and_fact_roundtrip(tmp_path):
    store = make_store(tmp_path)
    event = store.add_event("text", "test", "I prefer oat milk.")
    fact = store.add_fact("user", "prefers", "oat milk", provenance=[event.id])

    assert fact.id.startswith("fact_")
    assert fact.provenance == [event.id]
    results = store.search("oat milk")
    assert results
    assert results[0].obj == "oat milk"
    store.close()


def test_duplicate_fact_reinforces(tmp_path):
    store = make_store(tmp_path)
    first = store.add_fact("user", "prefers", "oat milk", confidence=0.6)
    second = store.add_fact("user", "prefers", "oat milk", confidence=0.7)

    assert first.id == second.id
    assert second.confidence > first.confidence
    assert len(store.list_facts(status="active")) == 1
    store.close()


def test_decay_marks_fading_and_archived(tmp_path):
    store = make_store(tmp_path)
    store.add_fact(
        "user",
        "lives_in",
        "Berlin",
        recorded_at="2026-01-01T00:00:00+00:00",
        last_seen="2026-01-01T00:00:00+00:00",
    )
    changed = store.decay(now="2026-02-15T00:00:00+00:00")
    assert changed == 1
    assert store.list_facts(status="fading")[0].obj == "Berlin"

    store.decay(now="2026-04-01T00:00:00+00:00")
    assert store.list_facts(status="archived")[0].obj == "Berlin"
    store.close()


def test_forget_removes_fact(tmp_path):
    store = make_store(tmp_path)
    fact = store.add_fact("user", "prefers", "oat milk")
    assert store.forget(fact.id) is True
    assert store.search("oat milk") == []
    assert store.get_fact(fact.id) is None
    store.close()


def test_paper_contains_fact(tmp_path):
    store = make_store(tmp_path)
    store.add_fact("user", "prefers", "oat milk", confidence=0.8)
    text = Paper(store).render("2026-11-13", store.stats())
    assert "The Morning Paper" in text
    assert "oat milk" in text
    store.close()


def test_sleep_builds_brief(tmp_path):
    store = make_store(tmp_path)
    store.add_event(
        "text",
        "test",
        "I prefer oat milk.",
        captured_at="2026-11-13T12:00:00+00:00",
    )
    cycle = SleepCycle(store, HeuristicExtractor(), Paper(store))
    brief = cycle.run(day=date(2026, 11, 13))

    assert "oat milk" in brief.markdown
    assert store.latest_brief() is not None
    assert store.search("oat milk")
    store.close()


def test_mcp_lists_tools(tmp_path):
    store = make_store(tmp_path)
    response = handle(store, {"jsonrpc": "2.0", "id": 1, "method": "tools/list"})
    assert response is not None
    assert len(response["result"]["tools"]) == 4
    store.close()
