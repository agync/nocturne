.PHONY: test lint fmt run

test:
	uv run pytest -q

lint:
	uv run ruff check .

fmt:
	uv run ruff format .

run:
	uv run nocturne --help
