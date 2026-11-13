"""A small MCP stdio server for the memory tools.

The transport is newline-delimited JSON-RPC 2.0, which is enough for a local
client. Replace it with the official MCP SDK when the tool surface grows.
"""

from __future__ import annotations

import json
import sys
from typing import Any

from . import __version__
from .store import Store

TOOLS: list[dict[str, Any]] = [
    {
        "name": "memory_search",
        "description": "Search the fact graph and return cited facts.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "limit": {"type": "integer", "default": 10},
            },
            "required": ["query"],
        },
    },
    {
        "name": "memory_remember",
        "description": "Add one text event to the tape.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "text": {"type": "string"},
                "source": {"type": "string", "default": "mcp"},
            },
            "required": ["text"],
        },
    },
    {
        "name": "memory_forget",
        "description": "Delete one fact and its links.",
        "inputSchema": {
            "type": "object",
            "properties": {"fact_id": {"type": "string"}},
            "required": ["fact_id"],
        },
    },
    {
        "name": "memory_brief",
        "description": "Return the latest morning paper.",
        "inputSchema": {"type": "object", "properties": {}},
    },
]


def _result(request_id: Any, result: dict[str, Any]) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": request_id, "result": result}


def _error(request_id: Any, code: int, message: str) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": request_id, "error": {"code": code, "message": message}}


def call_tool(store: Store, name: str, arguments: dict[str, Any]) -> str:
    if name == "memory_search":
        facts = store.search(str(arguments.get("query", "")), int(arguments.get("limit", 10)))
        if not facts:
            return "no facts"
        return "\n".join(
            f"{fact.id[:8]}  {fact.subject} {fact.predicate} {fact.obj} "
            f"({fact.confidence:.2f}, {fact.status})"
            for fact in facts
        )
    if name == "memory_remember":
        event = store.add_event("text", str(arguments.get("source", "mcp")), str(arguments["text"]))
        return event.id
    if name == "memory_forget":
        return "forgotten" if store.forget(str(arguments["fact_id"])) else "not found"
    if name == "memory_brief":
        brief = store.latest_brief()
        return brief.markdown if brief else "no brief yet"
    raise ValueError(f"unknown tool {name}")


def handle(store: Store, request: dict[str, Any]) -> dict[str, Any] | None:
    method = request.get("method")
    request_id = request.get("id")

    if method == "initialize":
        return _result(
            request_id,
            {
                "protocolVersion": "2025-06-18",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "nocturne", "version": __version__},
            },
        )
    if method == "notifications/initialized":
        return None
    if method == "tools/list":
        return _result(request_id, {"tools": TOOLS})
    if method == "tools/call":
        params = request.get("params", {})
        try:
            text = call_tool(
                store,
                str(params.get("name", "")),
                dict(params.get("arguments", {})),
            )
            return _result(
                request_id,
                {"content": [{"type": "text", "text": text}], "isError": False},
            )
        except Exception as exc:  # noqa: BLE001
            return _result(
                request_id,
                {"content": [{"type": "text", "text": str(exc)}], "isError": True},
            )
    return _error(request_id, -32601, f"method not found: {method}")


def serve(store: Store) -> None:
    """Read JSON-RPC messages from stdin and write replies to stdout."""
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except json.JSONDecodeError:
            continue
        response = handle(store, request)
        if response is not None:
            sys.stdout.write(json.dumps(response) + "\n")
            sys.stdout.flush()
