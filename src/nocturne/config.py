"""Configuration from the environment."""

from __future__ import annotations

import os
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True, slots=True)
class Config:
    home: Path
    db_path: Path
    lemonade_url: str
    lemonade_key: str
    cortex_model: str
    hippocampus_model: str
    embed_model: str


def load() -> Config:
    home = Path(os.environ.get("NOCTURNE_HOME", "~/.nocturne")).expanduser()
    db_path = Path(os.environ.get("NOCTURNE_DB", str(home / "nocturne.sqlite3"))).expanduser()
    return Config(
        home=home,
        db_path=db_path,
        lemonade_url=os.environ.get(
            "LEMONADE_BASE_URL", "http://localhost:13305/api/v1"
        ).rstrip("/"),
        lemonade_key=os.environ.get("LEMONADE_API_KEY", "lemonade"),
        cortex_model=os.environ.get(
            "NOCTURNE_CORTEX_MODEL", "Qwen3.6-35B-A3B-MTP-GGUF"
        ),
        hippocampus_model=os.environ.get(
            "NOCTURNE_HIPPOCAMPUS_MODEL", "gemma4-it:e4b"
        ),
        embed_model=os.environ.get(
            "NOCTURNE_EMBED_MODEL", "ggml-org/embeddinggemma-300M-GGUF:Q8_0"
        ),
    )
