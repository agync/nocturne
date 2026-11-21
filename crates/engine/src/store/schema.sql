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
-- A forget stamps the items that said what was forgotten: a read that began before it must not
-- write, and a later read must not say the same thing again. Claims are kept as hashes, so the
-- forgotten words are not left in the store.
ALTER TABLE items ADD COLUMN forget_epoch INTEGER NOT NULL DEFAULT 0;
ALTER TABLE items ADD COLUMN lease_epoch INTEGER NOT NULL DEFAULT 0;
CREATE TABLE forgotten_claims(
  item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
  claim TEXT NOT NULL,
  PRIMARY KEY(item_id, claim));

-- migration 7
-- Away from home: paired phones, what they kept, what they did (never content), and the
-- feature's own settings and counters. A removed phone keeps its row (revoked) so receipts
-- still name it; delete everything removes every row.
CREATE TABLE remote_devices(
  id TEXT PRIMARY KEY, name TEXT NOT NULL, public_key BLOB NOT NULL, home_origin TEXT NOT NULL,
  connection TEXT NOT NULL, address TEXT, paired_at TEXT NOT NULL, last_used_at TEXT, revoked_at TEXT);
CREATE TABLE remote_keeps(
  item_id TEXT PRIMARY KEY REFERENCES items(id) ON DELETE CASCADE,
  device_id TEXT NOT NULL REFERENCES remote_devices(id),
  client_id TEXT NOT NULL, kept_at TEXT NOT NULL,
  UNIQUE(device_id, client_id));
CREATE TABLE remote_events(
  id INTEGER PRIMARY KEY, device_id TEXT, action TEXT NOT NULL, result TEXT NOT NULL,
  bytes REAL NOT NULL DEFAULT 0, count INTEGER NOT NULL DEFAULT 1, at TEXT NOT NULL);
CREATE INDEX remote_events_at ON remote_events(at);
CREATE TABLE remote_state(
  id INTEGER PRIMARY KEY CHECK (id = 1), connection TEXT NOT NULL DEFAULT 'simple',
  wanted_on INTEGER NOT NULL DEFAULT 0, auto_off TEXT NOT NULL DEFAULT 'unused30Days',
  keep_awake INTEGER NOT NULL DEFAULT 0, sent_bytes REAL NOT NULL DEFAULT 0,
  first_on_at TEXT, on_since TEXT, last_on_at TEXT, last_off_at TEXT,
  address TEXT, address_since TEXT, machine_name TEXT);
INSERT INTO remote_state(id) VALUES (1);

-- migration 8
-- What each paired phone kept today, in bytes, so its daily limit holds across a restart.
ALTER TABLE remote_devices ADD COLUMN day TEXT;
ALTER TABLE remote_devices ADD COLUMN day_bytes INTEGER NOT NULL DEFAULT 0;

-- migration 9
-- Renaming, merging and re-filing. A name the person set is kept with the names it replaced, so
-- a re-read files claims under the name they chose. A merged thing keeps its row (hidden) and
-- every change is logged, so each can be taken back. A memory the person moved remembers what
-- it was first filed under, so a re-read finds it instead of making it again.
ALTER TABLE entities ADD COLUMN merged_into TEXT REFERENCES entities(id);
ALTER TABLE memories ADD COLUMN moved_from TEXT;
ALTER TABLE memories ADD COLUMN moved_from_key TEXT;
CREATE INDEX memories_moved_from ON memories(moved_from_key) WHERE moved_from_key IS NOT NULL;
CREATE TABLE entity_names(
  id INTEGER PRIMARY KEY, entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  name TEXT NOT NULL, key TEXT NOT NULL, change_id TEXT, at TEXT NOT NULL);
CREATE INDEX entity_names_key ON entity_names(key);
CREATE INDEX entity_names_entity ON entity_names(entity_id);
CREATE TABLE entity_changes(
  id TEXT PRIMARY KEY, kind TEXT NOT NULL, at TEXT NOT NULL, undone_at TEXT, detail TEXT NOT NULL);

-- migration 10
-- Online answers: the feature's own settings (never the key) and every question it sent, kept as
-- a template in which each memory sentence is replaced by its memory id, with a hash of the exact
-- line sent (`memory_lines`, JSON `[{"id", "hash"}]`). The template keeps the rest of each line
-- (dates, what it was before). A line is shown again only while the memory still hashes the same,
-- so a forgotten or re-read memory's words sent are not kept here. Choosing the model
-- for each job: which recall model made each vector, and the vectors a re-read is building.
CREATE TABLE online_state(
  id INTEGER PRIMARY KEY CHECK (id = 1), wanted_on INTEGER NOT NULL DEFAULT 0,
  set_up INTEGER NOT NULL DEFAULT 0, base_url TEXT, model TEXT, phones INTEGER NOT NULL DEFAULT 1,
  send_sensitive INTEGER NOT NULL DEFAULT 0, limit_kind TEXT NOT NULL DEFAULT 'none',
  limit_questions INTEGER NOT NULL DEFAULT 200, limit_usd REAL, pause TEXT, problem TEXT,
  problem_since TEXT, sent_bytes REAL NOT NULL DEFAULT 0, first_on_at TEXT, on_since TEXT,
  last_on_at TEXT, last_off_at TEXT, key_storage TEXT);
INSERT INTO online_state(id) VALUES (1);
CREATE TABLE online_sends(
  id INTEGER PRIMARY KEY, at TEXT NOT NULL, kind TEXT NOT NULL, host TEXT NOT NULL, model TEXT NOT NULL,
  question TEXT NOT NULL, template TEXT NOT NULL, memory_lines TEXT NOT NULL DEFAULT '[]',
  bytes_up REAL NOT NULL DEFAULT 0, bytes_down REAL NOT NULL DEFAULT 0, cost_usd REAL,
  status INTEGER, seconds REAL, result TEXT NOT NULL, reason TEXT, counted INTEGER NOT NULL DEFAULT 0);
CREATE INDEX online_sends_at ON online_sends(at);
ALTER TABLE embeddings ADD COLUMN model TEXT NOT NULL DEFAULT 'Qwen3-Embedding-0.6B-GGUF';
CREATE TABLE embeddings_next(
  memory_id TEXT PRIMARY KEY REFERENCES memories(id) ON DELETE CASCADE,
  model TEXT NOT NULL, dim INTEGER NOT NULL, vec BLOB NOT NULL);
CREATE TABLE reindex_state(
  id INTEGER PRIMARY KEY CHECK (id = 1), model TEXT, from_model TEXT, dim INTEGER,
  total INTEGER NOT NULL DEFAULT 0, started_at TEXT, finished_at TEXT, phase TEXT);
INSERT INTO reindex_state(id) VALUES (1);

-- migration 11
-- Word endings no longer hide a match when answering: "plumbers" finds "plumber". Stemmed
-- copies of both full-text indexes, filled from what the tables hold and kept current by
-- triggers. The indexes searched as the person types stay unstemmed, so a word part way typed
-- still matches.
CREATE VIRTUAL TABLE items_stem USING fts5(
  title, text, summary, content='items', content_rowid='rowid', tokenize='porter unicode61');
CREATE VIRTUAL TABLE memories_stem USING fts5(
  sentence, subject, object, content='memories', content_rowid='rowid',
  tokenize='porter unicode61');
INSERT INTO items_stem(items_stem, rank) VALUES('secure-delete', 1);
INSERT INTO memories_stem(memories_stem, rank) VALUES('secure-delete', 1);
INSERT INTO items_stem(items_stem) VALUES('rebuild');
INSERT INTO memories_stem(memories_stem) VALUES('rebuild');

CREATE TRIGGER items_stem_ai AFTER INSERT ON items BEGIN
  INSERT INTO items_stem(rowid, title, text, summary)
  VALUES (new.rowid, new.title, new.text, new.summary);
END;
CREATE TRIGGER items_stem_ad AFTER DELETE ON items BEGIN
  INSERT INTO items_stem(items_stem, rowid, title, text, summary)
  VALUES ('delete', old.rowid, old.title, old.text, old.summary);
END;
CREATE TRIGGER items_stem_au AFTER UPDATE OF title, text, summary ON items BEGIN
  INSERT INTO items_stem(items_stem, rowid, title, text, summary)
  VALUES ('delete', old.rowid, old.title, old.text, old.summary);
  INSERT INTO items_stem(rowid, title, text, summary)
  VALUES (new.rowid, new.title, new.text, new.summary);
END;

CREATE TRIGGER memories_stem_ai AFTER INSERT ON memories BEGIN
  INSERT INTO memories_stem(rowid, sentence, subject, object)
  VALUES (new.rowid, new.sentence, new.subject, new.object);
END;
CREATE TRIGGER memories_stem_ad AFTER DELETE ON memories BEGIN
  INSERT INTO memories_stem(memories_stem, rowid, sentence, subject, object)
  VALUES ('delete', old.rowid, old.sentence, old.subject, old.object);
END;
CREATE TRIGGER memories_stem_au AFTER UPDATE OF sentence, subject, object ON memories BEGIN
  INSERT INTO memories_stem(memories_stem, rowid, sentence, subject, object)
  VALUES ('delete', old.rowid, old.sentence, old.subject, old.object);
  INSERT INTO memories_stem(rowid, sentence, subject, object)
  VALUES (new.rowid, new.sentence, new.subject, new.object);
END;

-- migration 12
-- The journal (docs/design/journaling.md). Journal roles, days, blocks and versions live in
-- items.metadata (`journal`, `journal_day`, `block`, `replaces`, `about`, `feeling`, `line`,
-- `urge`, `tried`, `photo`, `alt`).
ALTER TABLE items ADD COLUMN left_out INTEGER NOT NULL DEFAULT 0;
-- An entry block is read once the person has stopped writing for a while.
ALTER TABLE items ADD COLUMN hold_until TEXT;
CREATE INDEX items_journal_day ON items(json_extract(metadata, '$.journal_day'))
  WHERE json_extract(metadata, '$.journal_day') IS NOT NULL;
-- The third sensitivity: read from an entry or a good thing. Kept out of audio, notifications,
-- the tray and a phone's lock screen.
ALTER TABLE memories ADD COLUMN journal INTEGER NOT NULL DEFAULT 0;
CREATE TABLE mentions(
  id TEXT PRIMARY KEY, item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
  entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE, name TEXT NOT NULL,
  start INTEGER, end INTEGER, ord INTEGER NOT NULL, state TEXT NOT NULL DEFAULT 'found',
  new INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, UNIQUE(item_id, entity_id));
CREATE INDEX mentions_entity ON mentions(entity_id);
-- Reflections and recaps: derived, one per period, deleted with Delete everything.
CREATE TABLE reflections(
  id TEXT PRIMARY KEY, kind TEXT NOT NULL, start TEXT NOT NULL, end TEXT NOT NULL,
  written_at TEXT NOT NULL, written TEXT NOT NULL, so_far INTEGER NOT NULL,
  from_count INTEGER NOT NULL, lines_removed INTEGER NOT NULL DEFAULT 0, body TEXT NOT NULL,
  UNIQUE(kind, start));
CREATE TABLE reflection_cites(
  reflection_id TEXT NOT NULL REFERENCES reflections(id) ON DELETE CASCADE,
  item_id TEXT NOT NULL, PRIMARY KEY(reflection_id, item_id));
CREATE INDEX reflection_cites_item ON reflection_cites(item_id);
-- "Which {name} is this?": the reading held back until the person answers.
CREATE TABLE name_questions(
  id TEXT PRIMARY KEY, item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
  name TEXT NOT NULL, quote TEXT NOT NULL, options TEXT NOT NULL, candidates TEXT NOT NULL,
  created_at TEXT NOT NULL, answered_at TEXT, answer TEXT);

-- migration 13
-- The originals a forget deletes, as its preview listed them, so nothing else goes with them.
ALTER TABLE forget_tokens ADD COLUMN items TEXT;

-- migration 14
-- The page's question from the journal: the block its sentence came from, as JSON.
ALTER TABLE questions ADD COLUMN journal TEXT;
-- A reflection or recap the person deleted: not written again until they ask for it.
CREATE TABLE reflections_declined(
  kind TEXT NOT NULL, start TEXT NOT NULL, declined_at TEXT NOT NULL, PRIMARY KEY(kind, start));

-- migration 15
-- The decision log (decision 0016): what a rule said and what a decider said about the same
-- thing, side by side, for the evaluation to compare. Derived and local: a row goes with the item
-- or memories it is about, and with Delete everything.
CREATE TABLE decisions(
  id TEXT PRIMARY KEY, kind TEXT NOT NULL,
  item_id TEXT REFERENCES items(id) ON DELETE CASCADE,
  memory_id TEXT REFERENCES memories(id) ON DELETE CASCADE,
  other_id TEXT REFERENCES memories(id) ON DELETE CASCADE,
  state TEXT NOT NULL, rule INTEGER NOT NULL, rule_detail TEXT, created_at TEXT NOT NULL,
  asked_at TEXT, decider TEXT, label TEXT, probabilities TEXT, p REAL, verdict INTEGER,
  ms INTEGER, error TEXT);
CREATE UNIQUE INDEX decisions_key
  ON decisions(kind, COALESCE(item_id, ''), COALESCE(memory_id, ''), COALESCE(other_id, ''));
CREATE INDEX decisions_item ON decisions(item_id);
CREATE INDEX decisions_memory ON decisions(memory_id);
CREATE INDEX decisions_other ON decisions(other_id);
CREATE INDEX decisions_waiting ON decisions(created_at) WHERE asked_at IS NULL;
CREATE INDEX decisions_created ON decisions(created_at);
-- Every item whose words a row's state copies. When one goes (forgotten, undone, an earlier
-- version replaced), the row goes with it.
CREATE TABLE decision_items(
  decision_id TEXT NOT NULL REFERENCES decisions(id) ON DELETE CASCADE,
  item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
  PRIMARY KEY(decision_id, item_id));
CREATE INDEX decision_items_item ON decision_items(item_id);
CREATE TRIGGER decision_items_ad AFTER DELETE ON decision_items BEGIN
  DELETE FROM decisions WHERE id = old.decision_id;
END;

-- migration 16
-- What paired phone apps bring (docs/design/phone-apps.md, phone-health.md). Calendar entries
-- and health readings are items (kinds `calendar` and `reading`); what they hold lives in
-- items.metadata (`phone`, `calendar`, `reading`, `day`, `replaces`). Each source of each phone
-- has a row here, and each entry or reading the phone's own id for it and the item holding its
-- current version, so a resend is kept once and a change is a new version.
CREATE TABLE phone_sources(
  device_id TEXT NOT NULL, source TEXT NOT NULL, taking INTEGER NOT NULL DEFAULT 1,
  stopped_at TEXT, last_in_at TEXT, last_count INTEGER NOT NULL DEFAULT 0,
  calendars TEXT NOT NULL DEFAULT '[]', sync_id TEXT, sync_taken INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY(device_id, source));
CREATE TABLE phone_records(
  device_id TEXT NOT NULL, source TEXT NOT NULL, phone_id TEXT NOT NULL,
  item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE, version INTEGER NOT NULL,
  removed_at TEXT, came_in_at TEXT NOT NULL, UNIQUE(device_id, source, phone_id));
CREATE INDEX phone_records_item ON phone_records(item_id);
CREATE INDEX items_reading_day ON items(json_extract(metadata, '$.reading.type'),
  json_extract(metadata, '$.day')) WHERE kind = 'reading';
CREATE INDEX items_phone_source ON items(json_extract(metadata, '$.phone.source'))
  WHERE json_extract(metadata, '$.phone.source') IS NOT NULL;
-- What a phone delete covers, so applying it can also stop that source.
ALTER TABLE forget_tokens ADD COLUMN phone_scope TEXT;
-- Memories a delete keeps because something else also said them: only their links to the
-- deleted items go.
ALTER TABLE forget_tokens ADD COLUMN unlink TEXT;
-- Which app and version a phone runs, once it says so.
ALTER TABLE remote_devices ADD COLUMN app_platform TEXT;
ALTER TABLE remote_devices ADD COLUMN app_version TEXT;
-- Earlier versions are found by what replaced them.
CREATE INDEX items_replaces ON items(json_extract(metadata, '$.replaces'))
  WHERE json_extract(metadata, '$.replaces') IS NOT NULL;

-- migration 17
-- The newest phone build a phone and this computer both spoke when it last introduced itself;
-- below the oldest this computer speaks, the phone must update before it can connect.
ALTER TABLE remote_devices ADD COLUMN agreed_build INTEGER;
