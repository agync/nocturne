"""Command line for Nocturne."""

from __future__ import annotations

import argparse
from datetime import date as date_cls
from pathlib import Path

from .config import Config, load
from .extract import HeuristicExtractor, LemonadeExtractor
from .lemonade import LemonadeClient
from .mcp import serve
from .paper import Paper
from .sleep import SleepCycle
from .store import Store


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="nocturne", description=__doc__)
    parser.add_argument(
        "--lemonade",
        action="store_true",
        help="use Lemonade for fact extraction instead of the offline heuristic",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    sub.add_parser("init", help="create the database")

    remember = sub.add_parser("remember", help="add one event")
    remember.add_argument("text", nargs="?", default="", help="the text to remember")
    remember.add_argument("--file", help="read the text from a file")
    remember.add_argument("--kind", default="text", help="text, page, note, or other")
    remember.add_argument("--source", default="cli")

    ask = sub.add_parser("ask", help="search the fact graph")
    ask.add_argument("query")
    ask.add_argument("--limit", type=int, default=10)

    sleep = sub.add_parser("sleep", help="run the nightly consolidation")
    sleep.add_argument("--date", help="YYYY-MM-DD, defaults to today")
    sleep.add_argument("--fade-days", type=int, default=30)
    sleep.add_argument("--archive-days", type=int, default=90)

    paper = sub.add_parser("paper", help="print the latest paper")
    paper.add_argument("--audio", help="write a spoken copy to this path")

    sub.add_parser("stats", help="show counts")

    forget = sub.add_parser("forget", help="delete one fact")
    forget.add_argument("fact_id")

    sub.add_parser("mcp", help="serve the memory tools over MCP stdio")

    return parser


def _extractor(config: Config, use_lemonade: bool):
    if not use_lemonade:
        return HeuristicExtractor()
    client = LemonadeClient(config.lemonade_url, config.lemonade_key)
    return LemonadeExtractor(client, config.cortex_model)


def _client(config: Config) -> LemonadeClient:
    return LemonadeClient(config.lemonade_url, config.lemonade_key)


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    config = load()

    if args.command == "init":
        config.home.mkdir(parents=True, exist_ok=True)
        store = Store(config.db_path)
        store.close()
        print(f"initialised {config.db_path}")
        return 0

    store = Store(config.db_path)

    if args.command == "remember":
        if args.file:
            content = Path(args.file).read_text(encoding="utf-8")
            source = args.source if args.source != "cli" else str(args.file)
        else:
            content = args.text
            source = args.source
        if not content.strip():
            print("nothing to remember")
            return 2
        event = store.add_event(args.kind, source, content)
        print(f"{event.id} {event.content_hash[:12]}")
        return 0

    if args.command == "ask":
        facts = store.search(args.query, limit=args.limit)
        if not facts:
            print("no facts")
            return 1
        for fact in facts:
            print(
                f"{fact.id[:8]}  {fact.subject} {fact.predicate} {fact.obj}  "
                f"({fact.confidence:.2f}, {fact.status})"
            )
        return 0

    if args.command == "sleep":
        target = date_cls.fromisoformat(args.date) if args.date else None
        cycle = SleepCycle(
            store,
            _extractor(config, args.lemonade),
            Paper(store, _client(config)),
        )
        brief = cycle.run(
            day=target,
            fade_days=args.fade_days,
            archive_days=args.archive_days,
        )
        print(brief.markdown)
        return 0

    if args.command == "paper":
        brief = store.latest_brief()
        if brief is None:
            cycle = SleepCycle(
                store,
                _extractor(config, args.lemonade),
                Paper(store, _client(config)),
            )
            brief = cycle.run()
        print(brief.markdown)
        if args.audio:
            paper = Paper(store, _client(config))
            Path(args.audio).write_bytes(paper.render_audio(brief.markdown))
            print(f"wrote {args.audio}")
        return 0

    if args.command == "stats":
        for key, value in store.stats().items():
            print(f"{key}: {value}")
        return 0

    if args.command == "forget":
        if store.forget(args.fact_id):
            print("forgotten")
            return 0
        print("not found")
        return 1

    if args.command == "mcp":
        serve(store)
        return 0

    parser.error(f"unknown command {args.command}")
    return 2
