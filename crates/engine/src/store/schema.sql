-- migration 1
CREATE TABLE items(
  id TEXT PRIMARY KEY, kind TEXT NOT NULL, source TEXT NOT NULL, trust TEXT NOT NULL,
  sha256 TEXT NOT NULL UNIQUE, ext TEXT, mime TEXT, file_name TEXT, title TEXT NOT NULL,
  kept_at TEXT NOT NULL, status TEXT NOT NULL, reason TEXT, text TEXT, summary TEXT,
  language TEXT, duration_ms INTEGER, pages INTEGER, origin TEXT,
  private INTEGER NOT NULL DEFAULT 0, lease_until TEXT, attempts INTEGER NOT NULL DEFAULT 0,
  understood_in_ms INTEGER, metadata TEXT NOT NULL DEFAULT '{}');
CREATE INDEX items_kept_at ON items(kept_at);
CREATE INDEX items_status ON items(status);

CREATE TABLE entities(
  id TEXT PRIMARY KEY, kind TEXT NOT NULL, name TEXT NOT NULL, key TEXT NOT NULL UNIQUE, role TEXT);

CREATE TABLE memories(
  id TEXT PRIMARY KEY, subject TEXT NOT NULL, subject_key TEXT NOT NULL,
  entity_id TEXT REFERENCES entities(id), predicate TEXT NOT NULL, object TEXT NOT NULL,
  object_key TEXT NOT NULL, sentence TEXT NOT NULL, valid_from TEXT, valid_to TEXT, due TEXT,
  recorded_at TEXT NOT NULL, superseded_at TEXT, superseded_by TEXT, confidence REAL NOT NULL,
  status TEXT NOT NULL, confirmed INTEGER NOT NULL DEFAULT 0, last_used TEXT, let_go_at TEXT,
  done INTEGER NOT NULL DEFAULT 0, keep_forever INTEGER NOT NULL DEFAULT 0,
  private INTEGER NOT NULL DEFAULT 0, suggest_private INTEGER NOT NULL DEFAULT 0,
  tags TEXT NOT NULL DEFAULT '[]',
  -- Set while a forget is under way: the row is already invisible everywhere.
  forgetting INTEGER NOT NULL DEFAULT 0);
CREATE INDEX memories_current ON memories(subject_key, predicate) WHERE superseded_at IS NULL;
CREATE INDEX memories_superseded_by ON memories(superseded_by) WHERE superseded_by IS NOT NULL;
CREATE INDEX memories_recorded_at ON memories(recorded_at);

CREATE TABLE memory_sources(
  memory_id TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
  item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
  span TEXT, PRIMARY KEY(memory_id, item_id));
CREATE INDEX memory_sources_item ON memory_sources(item_id);

CREATE TABLE conflicts(
  id TEXT PRIMARY KEY, a TEXT NOT NULL, b TEXT NOT NULL, created_at TEXT NOT NULL,
  resolved_at TEXT, kept TEXT);

CREATE TABLE embeddings(
  memory_id TEXT PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE,
  dim INTEGER NOT NULL, vec BLOB NOT NULL);

CREATE TABLE pages(
  date TEXT PRIMARY KEY, number INTEGER NOT NULL, window_start TEXT NOT NULL,
  window_end TEXT NOT NULL, headline TEXT NOT NULL, dek TEXT NOT NULL, variant TEXT NOT NULL,
  tidied_from TEXT, tidied_to TEXT, body TEXT NOT NULL);

CREATE TABLE page_lines(
  id TEXT PRIMARY KEY, page_date TEXT NOT NULL REFERENCES pages(date) ON DELETE CASCADE,
  section TEXT NOT NULL, kind TEXT NOT NULL, ord INTEGER NOT NULL, text TEXT NOT NULL,
  done INTEGER NOT NULL DEFAULT 0, forgotten INTEGER NOT NULL DEFAULT 0);
CREATE INDEX page_lines_page ON page_lines(page_date);

CREATE TABLE page_line_memories(
  line_id TEXT NOT NULL REFERENCES page_lines(id) ON DELETE CASCADE,
  memory_id TEXT NOT NULL, PRIMARY KEY(line_id, memory_id));
CREATE INDEX page_line_memories_memory ON page_line_memories(memory_id);

CREATE TABLE questions(
  page_date TEXT PRIMARY KEY, memory_id TEXT, kind TEXT NOT NULL, text TEXT NOT NULL,
  asked_at TEXT NOT NULL, answer TEXT, answered_at TEXT);

CREATE TABLE audio_cache(
  page_date TEXT NOT NULL, voice TEXT NOT NULL, path TEXT NOT NULL,
  PRIMARY KEY(page_date, voice));

CREATE TABLE ledger(
  id INTEGER PRIMARY KEY, kind TEXT NOT NULL, label TEXT NOT NULL, host TEXT,
  bytes REAL NOT NULL, at TEXT NOT NULL);

CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE forget_tokens(
  token TEXT PRIMARY KEY, ids TEXT NOT NULL, include_originals INTEGER NOT NULL,
  stamp TEXT NOT NULL, expires_at TEXT NOT NULL);

-- A forget that has started and must finish, even across a crash.
CREATE TABLE forget_jobs(
  id INTEGER PRIMARY KEY, ids TEXT NOT NULL, items TEXT NOT NULL,
  include_originals INTEGER NOT NULL);

CREATE VIRTUAL TABLE items_fts USING fts5(
  title, text, summary, content='items', content_rowid='rowid');
CREATE VIRTUAL TABLE memories_fts USING fts5(
  sentence, subject, object, content='memories', content_rowid='rowid');
-- Deleted terms are overwritten in the index, not left in free pages.
INSERT INTO items_fts(items_fts, rank) VALUES('secure-delete', 1);
INSERT INTO memories_fts(memories_fts, rank) VALUES('secure-delete', 1);

CREATE TRIGGER items_ai AFTER INSERT ON items BEGIN
  INSERT INTO items_fts(rowid, title, text, summary)
  VALUES (new.rowid, new.title, new.text, new.summary);
END;
CREATE TRIGGER items_ad AFTER DELETE ON items BEGIN
  INSERT INTO items_fts(items_fts, rowid, title, text, summary)
  VALUES ('delete', old.rowid, old.title, old.text, old.summary);
END;
CREATE TRIGGER items_au AFTER UPDATE OF title, text, summary ON items BEGIN
  INSERT INTO items_fts(items_fts, rowid, title, text, summary)
  VALUES ('delete', old.rowid, old.title, old.text, old.summary);
  INSERT INTO items_fts(rowid, title, text, summary)
  VALUES (new.rowid, new.title, new.text, new.summary);
END;

CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
  INSERT INTO memories_fts(rowid, sentence, subject, object)
  VALUES (new.rowid, new.sentence, new.subject, new.object);
END;
CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, sentence, subject, object)
  VALUES ('delete', old.rowid, old.sentence, old.subject, old.object);
END;
CREATE TRIGGER memories_au AFTER UPDATE OF sentence, subject, object ON memories BEGIN
  INSERT INTO memories_fts(memories_fts, rowid, sentence, subject, object)
  VALUES ('delete', old.rowid, old.sentence, old.subject, old.object);
  INSERT INTO memories_fts(rowid, sentence, subject, object)
  VALUES (new.rowid, new.sentence, new.subject, new.object);
END;

-- migration 2
-- Who holds the read of an item, and the files a started forget must still remove.
ALTER TABLE items ADD COLUMN lease_token TEXT;
ALTER TABLE forget_jobs ADD COLUMN files TEXT NOT NULL DEFAULT '[]';

-- migration 3
-- "Read now" ordering for the queue, and a memory the person asked to see again later.
ALTER TABLE items ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE memories ADD COLUMN snoozed_until TEXT;

-- migration 4
-- What started a job: a forget, or an undone keep (which also takes the item's own row).
ALTER TABLE forget_jobs ADD COLUMN kind TEXT NOT NULL DEFAULT 'forget';

-- migration 5
-- Jobs an undone keep started before jobs carried a kind. Nothing else marked them; a pending
-- job that deletes originals and names a single item is taken to be one. A regular forget of
-- that shape is treated the same, which only makes pages stricter about citing that item.
UPDATE forget_jobs SET kind = 'unkeep'
 WHERE kind = 'forget' AND include_originals = 1 AND json_array_length(items) = 1;

-- migration 6
-- The intelligence layer: failures counted apart from leases taken, the status an item had
-- before a re-read took it (so giving it back restores that status), a re-read mark that
-- survives restarts, and whether the person has opened a saved page or it was re-saved once.
ALTER TABLE items ADD COLUMN failures INTEGER NOT NULL DEFAULT 0;
ALTER TABLE items ADD COLUMN status_before TEXT;
ALTER TABLE items ADD COLUMN reread INTEGER NOT NULL DEFAULT 0;
ALTER TABLE pages ADD COLUMN opened_at TEXT;
ALTER TABLE pages ADD COLUMN resaves INTEGER NOT NULL DEFAULT 0;
