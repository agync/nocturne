mod common;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use common::*;
use nocturne_contract::{BackupPhase, Edition, Kept, SettingsPatch, Source};
use nocturne_engine::{
    BACKUP_EXT, BackupRun, Engine, Error, Predicate, check_backup, probe_backup, unpack_backup,
};
use sha2::{Digest, Sha256};

const PASS: &str = "a long enough passphrase";

/// Passphrase work for most test backups, so they open quickly. One test writes at the real one.
pub const TEST_WORK: u8 = 10;

fn no_cancel() -> AtomicBool {
    AtomicBool::new(false)
}

fn noop(_: BackupPhase, _: u64, _: u64) {}

fn write_backup(
    engine: &Engine,
    dest: &Path,
    extras: &[(&str, &Path)],
) -> nocturne_engine::Result<nocturne_contract::BackupReport> {
    let cancel = no_cancel();
    let run = BackupRun {
        extras,
        progress: &noop,
        cancel: &cancel,
        started: &|_| {},
        work_factor: Some(TEST_WORK),
    };
    engine.backup_to(dest, PASS, &run)
}

fn backup(f: &Fixture, dir: &Path) -> PathBuf {
    let dest = dir.join(format!("Nocturne backup.{BACKUP_EXT}"));
    write_backup(&f.engine, &dest, &[]).unwrap();
    dest
}

fn check(file: &Path) -> nocturne_engine::Result<nocturne_contract::BackupSummary> {
    let out = tempfile::tempdir().unwrap();
    let scratch = out.path().join("scratch");
    let summary = check_backup(file, PASS, &scratch, &noop, &no_cancel());
    assert!(!scratch.exists(), "a check keeps nothing");
    summary
}

/// Check a backup that is expected to be refused, and prove no scratch folder was left.
fn refused(file: &Path) -> Error {
    check(file).unwrap_err()
}

/// Write an age file whose plaintext is a tar with a manifest, a database and no originals.
fn build(file: &Path, manifest: &str, db: &[u8]) {
    fn add<W: std::io::Write>(tar: &mut tar::Builder<W>, name: &str, data: &[u8]) {
        let mut header = tar::Header::new_gnu();
        header.set_path(name).unwrap();
        header.set_size(data.len() as u64);
        header.set_mode(0o600);
        header.set_mtime(0);
        header.set_cksum();
        tar.append(&header, data).unwrap();
    }
    let file = std::fs::File::create(file).unwrap();
    let encryptor =
        age::Encryptor::with_user_passphrase(age::secrecy::SecretString::from(PASS.to_string()));
    let stream = encryptor.wrap_output(file).unwrap();
    let mut tar = tar::Builder::new(stream);
    add(&mut tar, "manifest.json", manifest.as_bytes());
    add(&mut tar, "store/nocturne.db", db);
    tar.finish().unwrap();
    tar.into_inner().unwrap().finish().unwrap();
}

/// A store at the given version, with one old page when it stops at version 1.
fn database(upto: u32, stamp: u32) -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nocturne.db");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        for (version, sql) in nocturne_engine::store::migrations() {
            if version > upto {
                break;
            }
            conn.execute_batch(sql).unwrap();
            conn.pragma_update(None, "user_version", version).unwrap();
        }
        if upto == 1 {
            conn.execute(
                "INSERT INTO pages(date, number, window_start, window_end, headline, dek, \
                 variant, body) VALUES ('2026-11-14', 1, '2026-11-13T22:30:00Z', \
                 '2026-11-14T06:30:00Z', 'An older morning', 'dek', 'quiet', 'body')",
                [],
            )
            .unwrap();
        }
        conn.pragma_update(None, "user_version", stamp).unwrap();
    }
    std::fs::read(&path).unwrap()
}

/// A database at the newest schema, plus whatever rows a test needs in it.
fn database_with_latest(extra: &str) -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nocturne.db");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        for (version, sql) in nocturne_engine::store::migrations() {
            conn.execute_batch(sql).unwrap();
            conn.pragma_update(None, "user_version", version).unwrap();
        }
        conn.execute_batch(extra).unwrap();
    }
    std::fs::read(&path).unwrap()
}

fn manifest_text(format: u32, store_version: u32) -> String {
    format!(
        "{{\"format\":{format},\"storeVersion\":{store_version},\"appVersion\":\"9.9.9\",\
         \"madeAt\":\"2026-11-13T07:00:00.000Z\",\"memories\":0,\"items\":0,\"pages\":0,\
         \"originals\":0}}"
    )
}

/// Everything in a backup's tar, decrypted, for looking for what should not be there.
fn payload(file: &Path, passphrase: &str) -> Vec<u8> {
    let input = std::fs::File::open(file).unwrap();
    let decryptor = age::Decryptor::new(input).unwrap();
    let mut identity =
        age::scrypt::Identity::new(age::secrecy::SecretString::from(passphrase.to_string()));
    identity.set_max_work_factor(19);
    let mut stream = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .unwrap();
    let mut all = Vec::new();
    stream.read_to_end(&mut all).unwrap();
    all
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn unpack(file: &Path) -> (tempfile::TempDir, nocturne_contract::BackupSummary) {
    let out = tempfile::tempdir().unwrap();
    let stage = out.path().join("stage");
    let cancel = no_cancel();
    let summary = unpack_backup(file, PASS, &stage, &noop, &cancel).unwrap();
    (out, summary)
}

#[test]
fn a_backup_round_trips_everything() {
    let f = fixture();
    let original: Vec<u8> = (0..5000u32).map(|i| (i * 7 % 251) as u8).collect();
    let src = tempfile::tempdir().unwrap();
    let photo = match f
        .engine
        .keep_file(
            &write_file(src.path(), "appointment card.jpg", &original),
            Source::Screenshot,
        )
        .unwrap()
    {
        Kept::New { item } => item.id,
        _ => unreachable!(),
    };
    f.engine
        .apply(
            &photo,
            vec![cand(
                "Dentist",
                Predicate::Scheduled,
                "Tuesday",
                "Your dentist appointment is Tuesday.",
            )],
        )
        .unwrap();
    keep(&f.engine, "a plain note");
    let (_, old) = learn(
        &f.engine,
        "Gran lives in Leeds",
        cand("Gran", Predicate::LivesIn, "Leeds", "Gran lives in Leeds."),
    );
    f.engine
        .correct(&old, nocturne_engine::CorrectPatch::object("York"))
        .unwrap();
    f.clock.set_str("2026-11-14T06:00:00Z");
    let page = f
        .engine
        .compose_page(date("2026-11-14"), Edition::Screen)
        .unwrap();
    f.engine.save_page(&page).unwrap();
    f.engine
        .update_settings(SettingsPatch {
            page_time: Some("06:30".into()),
            reminders: Some(false),
            ..Default::default()
        })
        .unwrap();

    let extras = tempfile::tempdir().unwrap();
    let selection = write_file(extras.path(), "selection.json", b"{\"keep\":[\"one\"]}");
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("chosen.nocturne");
    let report = write_backup(&f.engine, &dest, &[("engine/selection.json", &selection)]).unwrap();
    assert_eq!(
        (report.memories, report.pages, report.originals),
        (2, 1, 1),
        "{report:?}"
    );
    assert!(report.bytes > 0.0);
    assert_eq!(Path::new(&report.path), dest.as_path());

    let summary = check(&dest).unwrap();
    assert_eq!(
        (
            summary.memories,
            summary.items,
            summary.pages,
            summary.originals
        ),
        (2, 3, 1, 1),
        "{summary:?}"
    );
    assert_eq!(summary.app_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(
        summary.store_version,
        nocturne_engine::store::latest_version()
    );
    assert!(summary.made_at.starts_with("2026-11-14T06:00"));
    assert!(!summary.online_used && !summary.away_used);

    let (stage_dir, unpacked) = unpack(&dest);
    assert_eq!(unpacked.originals, 1);
    let stage = stage_dir.path().join("stage");
    let restored = Engine::open(&stage.join("store"), f.clock.clone()).unwrap();
    let memories = |e: &Engine| {
        let mut v: Vec<(String, String)> = e
            .memories(Default::default())
            .unwrap()
            .into_iter()
            .map(|m| (m.id, m.sentence))
            .collect();
        v.sort();
        v
    };
    assert_eq!(memories(&restored), memories(&f.engine));
    assert_eq!(memories(&restored).len(), 2);

    let items = |e: &Engine| {
        let mut v: Vec<(String, String, bool)> = e
            .items(Default::default())
            .unwrap()
            .into_iter()
            .map(|i| (i.id, i.title, i.has_original))
            .collect();
        v.sort();
        v
    };
    assert_eq!(items(&restored), items(&f.engine));
    assert!(
        items(&restored)
            .iter()
            .any(|(_, title, has)| title.ends_with("appointment card.jpg") && *has)
    );

    assert_eq!(
        restored.page(Some(date("2026-11-14"))).unwrap(),
        f.engine.page(Some(date("2026-11-14"))).unwrap()
    );
    assert_eq!(restored.settings().unwrap(), f.engine.settings().unwrap());

    let stored: Vec<_> = std::fs::read_dir(stage.join("store").join("originals"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(stored.len(), 1);
    assert_eq!(std::fs::read(&stored[0]).unwrap(), original);
    assert_eq!(
        std::fs::read(stage.join("engine").join("selection.json")).unwrap(),
        b"{\"keep\":[\"one\"]}"
    );
}

#[test]
fn originals_with_unusual_extensions_round_trip_to_the_paths_the_store_expects() {
    let f = fixture();
    let src = tempfile::tempdir().unwrap();
    let names = [
        "card.jpg-large".to_string(),
        "notes.c++".to_string(),
        format!("file.{}", "e".repeat(30)),
        "café.日本語".to_string(),
    ];
    let mut kept = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let bytes = format!("original number {i}").into_bytes();
        let item = match f
            .engine
            .keep_file(&write_file(src.path(), name, &bytes), Source::Drop)
            .unwrap()
        {
            Kept::New { item } => item,
            Kept::Duplicate { .. } => unreachable!(),
        };
        let before = item.original_path.clone().unwrap();
        kept.push((item.id, bytes, before));
    }

    let out = tempfile::tempdir().unwrap();
    let dest = backup(&f, out.path());
    assert_eq!(check(&dest).unwrap().originals, 4);
    let (stage_dir, summary) = unpack(&dest);
    assert_eq!(summary.originals, 4);

    let stage = stage_dir.path().join("stage");
    let restored = Engine::open(&stage.join("store"), f.clock.clone()).unwrap();
    for (id, bytes, before) in kept {
        let item = restored.get_item(&id).unwrap();
        let after = PathBuf::from(item.original_path.unwrap());
        assert_eq!(
            after.file_name(),
            Path::new(&before).file_name(),
            "the store expects the same name"
        );
        assert_eq!(std::fs::read(&after).unwrap(), bytes, "{after:?}");
    }
    assert_eq!(
        std::fs::read_dir(stage.join("store/originals"))
            .unwrap()
            .count(),
        4
    );
}

#[test]
fn a_written_backup_pins_the_scrypt_work_factor() {
    let f = fixture();
    keep(&f.engine, "a note");
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join(format!("Nocturne backup.{BACKUP_EXT}"));
    let cancel = no_cancel();
    let run = BackupRun {
        extras: &[],
        progress: &noop,
        cancel: &cancel,
        started: &|_| {},
        work_factor: None,
    };
    f.engine.backup_to(&dest, PASS, &run).unwrap();
    let bytes = std::fs::read(&dest).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    let line = text
        .lines()
        .find(|line| line.starts_with("-> scrypt "))
        .expect("a scrypt recipient");
    let factor: u8 = line
        .split_whitespace()
        .nth(3)
        .expect("a work factor")
        .parse()
        .unwrap();
    assert_eq!(factor, 18, "{line}");
    check(&dest).unwrap();
}

#[test]
fn a_restored_database_with_a_trigger_of_its_own_is_refused() {
    let f = fixture();
    let db = database_with_latest(
        "CREATE TRIGGER phone_home AFTER UPDATE ON remote_devices \
         BEGIN SELECT 1; END;",
    );
    let out = tempfile::tempdir().unwrap();
    let file = out.path().join("phone-home");
    build(
        &file,
        &manifest_text(1, nocturne_engine::store::latest_version()),
        &db,
    );
    assert!(matches!(refused(&file), Error::BackupDamaged));
    drop(f);
}

#[test]
fn a_restored_database_with_a_rewritten_trigger_is_refused() {
    let _f = fixture();
    let db = database_with_latest(
        "DROP TRIGGER items_ai; \
         CREATE TRIGGER items_ai AFTER INSERT ON items BEGIN DELETE FROM pages; END;",
    );
    let out = tempfile::tempdir().unwrap();
    let file = out.path().join("rewritten");
    build(
        &file,
        &manifest_text(1, nocturne_engine::store::latest_version()),
        &db,
    );
    assert!(matches!(refused(&file), Error::BackupDamaged));
}

/// Write an age file whose tar holds a long-name or extended header naming the manifest, then
/// the manifest under another name, then a good database.
fn build_with_name_header(file: &Path, kind: tar::EntryType) {
    fn add<W: std::io::Write>(
        tar: &mut tar::Builder<W>,
        kind: tar::EntryType,
        name: &str,
        data: &[u8],
    ) {
        let mut header = tar::Header::new_gnu();
        header.set_path(name).unwrap();
        header.set_size(data.len() as u64);
        header.set_mode(0o600);
        header.set_mtime(0);
        header.set_entry_type(kind);
        header.set_cksum();
        tar.append(&header, data).unwrap();
    }
    let manifest = manifest_text(1, nocturne_engine::store::latest_version());
    let name: Vec<u8> = match kind {
        tar::EntryType::GNULongName => b"manifest.json\0".to_vec(),
        _ => b"22 path=manifest.json\n".to_vec(),
    };
    let out = std::fs::File::create(file).unwrap();
    let encryptor =
        age::Encryptor::with_user_passphrase(age::secrecy::SecretString::from(PASS.to_string()));
    let stream = encryptor.wrap_output(out).unwrap();
    let mut tar = tar::Builder::new(stream);
    add(&mut tar, kind, "././@LongLink", &name);
    add(
        &mut tar,
        tar::EntryType::Regular,
        "renamed",
        manifest.as_bytes(),
    );
    add(
        &mut tar,
        tar::EntryType::Regular,
        "store/nocturne.db",
        &database_with_latest(""),
    );
    tar.finish().unwrap();
    tar.into_inner().unwrap().finish().unwrap();
}

#[test]
fn long_name_and_extended_headers_are_refused_not_followed() {
    let _f = fixture();
    let out = tempfile::tempdir().unwrap();
    for kind in [tar::EntryType::GNULongName, tar::EntryType::XHeader] {
        let file = out.path().join(format!("{kind:?}"));
        build_with_name_header(&file, kind);
        assert!(matches!(refused(&file), Error::BackupDamaged), "{kind:?}");
        let stage = out.path().join(format!("stage-{kind:?}"));
        let err = unpack_backup(&file, PASS, &stage, &noop, &no_cancel()).unwrap_err();
        assert!(matches!(err, Error::BackupDamaged), "{kind:?}: {err:?}");
        assert!(!stage.exists());
    }
}

#[test]
fn a_file_asking_for_more_passphrase_work_than_allowed_is_refused_unworked() {
    let f = fixture();
    keep(&f.engine, "a note");
    let out = tempfile::tempdir().unwrap();
    let dest = backup(&f, out.path());
    let mut bytes = std::fs::read(&dest).unwrap();
    let at = bytes
        .windows(10)
        .position(|w| w == b"-> scrypt ")
        .expect("a scrypt recipient");
    let end = at + bytes[at..].iter().position(|&b| b == b'\n').unwrap();
    assert_eq!(&bytes[end - 3..end], format!(" {TEST_WORK}").as_bytes());
    bytes[end - 2..end].copy_from_slice(b"20");
    let costly = out.path().join("costly.nocturne");
    std::fs::write(&costly, &bytes).unwrap();
    assert!(matches!(refused(&costly), Error::BackupDamaged));
}

#[test]
fn a_restored_database_with_a_sha_or_extension_that_is_a_path_is_refused() {
    let _f = fixture();
    let out = tempfile::tempdir().unwrap();

    let bad_sha = database_with_latest(
        "INSERT INTO items(id, kind, source, trust, sha256, ext, file_name, title, kept_at, \
         status) VALUES ('itm_bad', 'document', 'drop', 'trusted', '../../x', NULL, 'x.txt', \
         'x', '2026-11-13T07:00:00.000Z', 'kept');",
    );
    let sha_file = out.path().join("bad-sha");
    build(
        &sha_file,
        &manifest_text(1, nocturne_engine::store::latest_version()),
        &bad_sha,
    );
    assert!(matches!(refused(&sha_file), Error::BackupDamaged));

    let bad_ext = database_with_latest(&format!(
        "INSERT INTO items(id, kind, source, trust, sha256, ext, file_name, title, kept_at, \
         status) VALUES ('itm_bad', 'document', 'drop', 'trusted', '{}', '../x', 'x.txt', \
         'x', '2026-11-13T07:00:00.000Z', 'kept');",
        "a".repeat(64)
    ));
    let ext_file = out.path().join("bad-ext");
    build(
        &ext_file,
        &manifest_text(1, nocturne_engine::store::latest_version()),
        &bad_ext,
    );
    assert!(matches!(refused(&ext_file), Error::BackupDamaged));
}

#[test]
fn a_forget_job_from_a_backup_cannot_shred_a_file_outside_the_store() {
    let f = fixture();
    let outside_dir = tempfile::tempdir().unwrap();
    let outside = write_file(outside_dir.path(), "keep-me.txt", b"must survive");
    let files = serde_json::to_string(&[outside.to_string_lossy().to_string()]).unwrap();
    let db = database_with_latest(&format!(
        "INSERT INTO forget_jobs(ids, items, include_originals, files) \
         VALUES ('[]', '[]', 1, '{}');",
        files.replace('\'', "''")
    ));
    let out = tempfile::tempdir().unwrap();
    let file = out.path().join("outside-file");
    build(
        &file,
        &manifest_text(1, nocturne_engine::store::latest_version()),
        &db,
    );
    let (out, _) = unpack(&file);
    let stage = out.path().join("stage");
    let conn = rusqlite::Connection::open(stage.join("store/nocturne.db")).unwrap();
    let recorded: String = conn
        .query_row("SELECT files FROM forget_jobs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(recorded, "[]", "the restore drops the paths it was given");
    drop(conn);
    assert_eq!(std::fs::read(&outside).unwrap(), b"must survive");

    let _engine = Engine::open(&stage.join("store"), f.clock.clone()).unwrap();
    assert_eq!(std::fs::read(&outside).unwrap(), b"must survive");
}

#[test]
fn a_live_forget_job_cannot_shred_a_file_outside_the_originals_folder() {
    let f = fixture();
    let outside_dir = tempfile::tempdir().unwrap();
    let outside = write_file(outside_dir.path(), "keep-me.txt", b"must survive");
    let files = serde_json::to_string(&[outside.to_string_lossy().to_string()]).unwrap();
    {
        let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
        conn.execute(
            "INSERT INTO forget_jobs(ids, items, include_originals, files) \
             VALUES ('[]', '[]', 0, ?1)",
            [&files],
        )
        .unwrap();
    }

    let Fixture { dir, clock, engine } = f;
    drop(engine);
    let _engine = Engine::open(dir.path(), clock).unwrap();
    assert_eq!(std::fs::read(&outside).unwrap(), b"must survive");
    assert_eq!(sql_count(dir.path(), "SELECT COUNT(*) FROM forget_jobs"), 0);
}

#[test]
fn a_check_overwrites_the_scratch_database_before_removing_it() {
    let f = fixture();
    keep(&f.engine, "a note");
    let out = tempfile::tempdir().unwrap();
    let dest = backup(&f, out.path());
    let scratch = out.path().join("scratch");
    let spy = out.path().join("check-spy");
    let linked = std::cell::Cell::new(false);
    check_backup(
        &dest,
        PASS,
        &scratch,
        &|_, _, _| {
            if linked.get() {
                return;
            }
            let db = scratch.join("store/nocturne.db");
            if db.is_file() {
                std::fs::hard_link(&db, &spy).unwrap();
                linked.set(true);
            }
        },
        &no_cancel(),
    )
    .unwrap();
    assert!(linked.get(), "the scratch database was seen");
    assert!(!scratch.exists());
    let bytes = std::fs::read(&spy).unwrap();
    assert!(!bytes.is_empty());
    assert!(bytes.iter().all(|&b| b == 0), "the scratch was overwritten");
}

#[test]
fn a_cancelled_restore_overwrites_the_staged_original_before_removing_it() {
    let f = fixture();
    let original: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let src = tempfile::tempdir().unwrap();
    f.engine
        .keep_file(&write_file(src.path(), "card.jpg", &original), Source::Drop)
        .unwrap();
    let out = tempfile::tempdir().unwrap();
    let dest = backup(&f, out.path());
    let stage = out.path().join("stage");
    let spy = out.path().join("restore-spy");
    let cancel = no_cancel();
    let linked = std::cell::Cell::new(false);
    let err = unpack_backup(
        &dest,
        PASS,
        &stage,
        &|phase, _, _| {
            if phase != BackupPhase::Restoring || linked.get() {
                return;
            }
            let Ok(entries) = std::fs::read_dir(stage.join("store/originals")) else {
                return;
            };
            for entry in entries.flatten() {
                if entry.metadata().is_ok_and(|m| m.len() > 0) {
                    std::fs::hard_link(entry.path(), &spy).unwrap();
                    linked.set(true);
                    cancel.store(true, Ordering::SeqCst);
                    return;
                }
            }
        },
        &cancel,
    )
    .unwrap_err();
    assert!(matches!(err, Error::Cancelled), "{err:?}");
    assert!(linked.get(), "the staged original was seen");
    assert!(!stage.exists());
    let bytes = std::fs::read(&spy).unwrap();
    assert!(!bytes.is_empty());
    assert!(
        bytes.iter().all(|&b| b == 0),
        "the staged bytes were overwritten"
    );
}

#[test]
fn the_wrong_passphrase_is_refused() {
    let f = fixture();
    keep(&f.engine, "a note");
    let out = tempfile::tempdir().unwrap();
    let dest = backup(&f, out.path());
    let scratch = out.path().join("scratch");
    let cancel = no_cancel();
    let err = check_backup(&dest, "a different passphrase", &scratch, &noop, &cancel).unwrap_err();
    assert!(matches!(err, Error::WrongPassphrase), "{err:?}");
    assert!(!scratch.exists());
}

#[test]
fn damaged_and_truncated_files_are_refused() {
    let f = fixture();
    keep(&f.engine, "a note");
    let out = tempfile::tempdir().unwrap();
    let file = backup(&f, out.path());
    let bytes = std::fs::read(&file).unwrap();
    assert!(bytes.len() > 1024);

    let mut flipped = bytes.clone();
    let middle = flipped.len() / 2;
    flipped[middle] ^= 0xff;
    let flip = write_file(out.path(), "flipped", &flipped);
    assert!(matches!(refused(&flip), Error::BackupDamaged));

    let half = write_file(out.path(), "half", &bytes[..bytes.len() / 2]);
    assert!(matches!(refused(&half), Error::BackupDamaged));

    let cut = write_file(out.path(), "cut", &bytes[..bytes.len() - 10]);
    assert!(matches!(refused(&cut), Error::BackupDamaged));

    let words = write_file(
        out.path(),
        "words",
        b"the quick brown fox jumps over the lazy dog again and again",
    );
    assert!(matches!(refused(&words), Error::NotABackup));

    let empty = write_file(out.path(), "empty", b"");
    assert!(matches!(refused(&empty), Error::NotABackup));
}

#[test]
fn only_a_passphrase_age_file_is_a_nocturne_backup() {
    let f = fixture();
    keep(&f.engine, "a note");
    let out = tempfile::tempdir().unwrap();
    let file = backup(&f, out.path());
    probe_backup(&file).unwrap();

    let words = write_file(out.path(), "words", b"not a backup at all");
    assert!(matches!(probe_backup(&words), Err(Error::NotABackup)));
    let empty = write_file(out.path(), "empty", b"");
    assert!(matches!(probe_backup(&empty), Err(Error::NotABackup)));

    // A real age file, but locked to a key rather than a passphrase.
    let keyed = out.path().join("keyed.nocturne");
    let identity = age::x25519::Identity::generate();
    let recipient = identity.to_public();
    let out_file = std::fs::File::create(&keyed).unwrap();
    let encryptor =
        age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
            .unwrap();
    let stream = encryptor.wrap_output(out_file).unwrap();
    stream.finish().unwrap();
    assert!(matches!(probe_backup(&keyed), Err(Error::NotABackup)));
}

#[test]
fn a_newer_backup_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let latest = nocturne_engine::store::latest_version();

    let newer_format = dir.path().join("format");
    build(
        &newer_format,
        &manifest_text(2, latest),
        &database(latest, latest),
    );
    assert!(matches!(refused(&newer_format), Error::BackupTooNew));

    let newer_store = dir.path().join("store");
    build(
        &newer_store,
        &manifest_text(1, latest + 1),
        &database(latest, latest),
    );
    assert!(matches!(refused(&newer_store), Error::BackupTooNew));

    let newer_db = dir.path().join("db");
    build(
        &newer_db,
        &manifest_text(1, latest),
        &database(latest, latest + 1),
    );
    assert!(matches!(refused(&newer_db), Error::BackupTooNew));
}

#[test]
fn an_older_backup_is_brought_up_to_date_while_unpacking() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("old");
    build(&file, &manifest_text(1, 1), &database(1, 1));

    let (out, summary) = unpack(&file);
    assert_eq!(summary.store_version, 1);
    assert_eq!(summary.pages, 1);
    assert_eq!(summary.memories, 0);

    let conn = rusqlite::Connection::open(out.path().join("stage/store/nocturne.db")).unwrap();
    let version: u32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, nocturne_engine::store::latest_version());
    let headline: String = conn
        .query_row("SELECT headline FROM pages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(headline, "An older morning");
}

#[test]
fn cancelling_leaves_nothing_behind() {
    let f = fixture();
    keep(&f.engine, "a note");
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("chosen.nocturne");
    let partial = out.path().join("chosen.nocturne.partial");
    let cancel = no_cancel();
    let calls = std::cell::Cell::new(0u32);
    let run = BackupRun {
        extras: &[],
        progress: &|_, _, _| {
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                cancel.store(true, Ordering::SeqCst);
            }
        },
        cancel: &cancel,
        started: &|_| {},
        work_factor: Some(TEST_WORK),
    };
    let err = f.engine.backup_to(&dest, PASS, &run).unwrap_err();
    assert!(matches!(err, Error::Cancelled), "{err:?}");
    assert!(!dest.exists());
    assert!(!partial.exists());
    assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0);
    assert!(!f.dir.path().join("backup-tmp").exists());
}

#[test]
fn the_real_name_never_appears_before_the_backup_checks_out() {
    let f = fixture();
    let original: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let src = tempfile::tempdir().unwrap();
    f.engine
        .keep_file(
            &write_file(src.path(), "card.jpg", &original),
            Source::Screenshot,
        )
        .unwrap();
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("chosen.nocturne");
    let partial = out.path().join("chosen.nocturne.partial");
    let cancel = no_cancel();
    let started = std::cell::Cell::new(false);
    let checked = std::cell::Cell::new(0u32);
    let run = BackupRun {
        extras: &[],
        progress: &|phase, done, total| {
            if phase == BackupPhase::Saving && done > 0 && done < total {
                assert!(!dest.exists(), "the real name is not there yet");
                assert!(partial.exists(), "the partial is");
                checked.set(checked.get() + 1);
            }
        },
        cancel: &cancel,
        started: &|path| {
            assert!(path.exists());
            assert_eq!(path, partial.as_path());
            started.set(true);
        },
        work_factor: Some(TEST_WORK),
    };
    f.engine.backup_to(&dest, PASS, &run).unwrap();
    assert!(started.get(), "the shell was told the partial path");
    assert!(checked.get() > 0);
    assert!(dest.exists() && !partial.exists());
}

#[test]
fn a_large_original_is_streamed_in_chunks() {
    let f = fixture();
    let big: Vec<u8> = (0..12 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
    f.engine.keep_bytes(&big, "txt", Source::Paste).unwrap();

    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("chosen.nocturne");
    let seen = std::cell::RefCell::new(Vec::new());
    let cancel = no_cancel();
    let run = BackupRun {
        extras: &[],
        progress: &|phase, done, total| {
            if phase == BackupPhase::Saving {
                seen.borrow_mut().push((done, total));
            }
        },
        cancel: &cancel,
        started: &|_| {},
        work_factor: Some(TEST_WORK),
    };
    f.engine.backup_to(&dest, PASS, &run).unwrap();
    let seen = seen.into_inner();
    assert!(seen.len() > 12, "{} calls", seen.len());
    for pair in seen.windows(2) {
        assert!(pair[1].0 > pair[0].0, "{pair:?}");
        assert!(
            pair[1].0 - pair[0].0 <= 2 * 1024 * 1024,
            "a step of {} bytes",
            pair[1].0 - pair[0].0
        );
    }
    let total = seen.last().unwrap().1;
    assert_eq!(seen.last().unwrap().0, total, "ends at the total");

    let (out, _) = unpack(&dest);
    let stored: Vec<_> = std::fs::read_dir(out.path().join("stage/store/originals"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(stored.len(), 1);
    let restored = std::fs::read(&stored[0]).unwrap();
    assert_eq!(restored.len(), big.len());
    assert_eq!(Sha256::digest(&restored), Sha256::digest(&big));
}

#[test]
fn the_phases_are_saving_then_checking_and_checking_ends_at_its_total() {
    let f = fixture();
    keep(&f.engine, "a note");
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("chosen.nocturne");
    let seen = std::cell::RefCell::new(Vec::new());
    let cancel = no_cancel();
    let run = BackupRun {
        extras: &[],
        progress: &|phase, done, total| seen.borrow_mut().push((phase, done, total)),
        cancel: &cancel,
        started: &|_| {},
        work_factor: Some(TEST_WORK),
    };
    f.engine.backup_to(&dest, PASS, &run).unwrap();
    let seen = seen.into_inner();
    assert_eq!(seen.first().unwrap().0, BackupPhase::Saving);
    assert!(seen.iter().any(|s| s.0 == BackupPhase::Checking));
    let last_check = seen
        .iter()
        .rev()
        .find(|s| s.0 == BackupPhase::Checking)
        .unwrap();
    assert_eq!(last_check.1, last_check.2, "checking ends at its total");
    assert!(last_check.2 > 0);
    assert!(
        !seen
            .iter()
            .skip_while(|s| s.0 == BackupPhase::Saving)
            .any(|s| s.0 == BackupPhase::Saving),
        "saving never comes back after checking"
    );
}

#[test]
fn cancelling_while_checking_leaves_nothing_behind() {
    let f = fixture();
    keep(&f.engine, "a note");
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("chosen.nocturne");
    let cancel = no_cancel();
    let run = BackupRun {
        extras: &[],
        progress: &|phase, _, _| {
            if phase == BackupPhase::Checking {
                cancel.store(true, Ordering::SeqCst);
            }
        },
        cancel: &cancel,
        started: &|_| {},
        work_factor: Some(TEST_WORK),
    };
    let err = f.engine.backup_to(&dest, PASS, &run).unwrap_err();
    assert!(matches!(err, Error::Cancelled), "{err:?}");
    assert!(!dest.exists());
    assert!(!out.path().join("chosen.nocturne.partial").exists());
    assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0);
    assert!(!f.dir.path().join("backup-tmp").exists());
}

#[test]
fn checking_keeps_no_folder_and_extracts_no_originals() {
    let f = fixture();
    let src = tempfile::tempdir().unwrap();
    f.engine
        .keep_file(
            &write_file(src.path(), "card.jpg", b"JPEG bytes of the card"),
            Source::Screenshot,
        )
        .unwrap();
    let out = tempfile::tempdir().unwrap();
    let dest = backup(&f, out.path());
    let scratch = out.path().join("scratch");
    let cancel = no_cancel();
    let seen = std::cell::Cell::new(0u32);
    let summary = check_backup(
        &dest,
        PASS,
        &scratch,
        &|phase, _, _| {
            if phase == BackupPhase::Checking {
                assert!(
                    !scratch.join("store").join("originals").exists(),
                    "a check writes no originals"
                );
                seen.set(seen.get() + 1);
            }
        },
        &cancel,
    )
    .unwrap();
    assert_eq!(summary.originals, 1);
    assert!(seen.get() > 0);
    assert!(!scratch.exists(), "the scratch folder is gone");
}

#[test]
fn secrets_and_connection_state_do_not_travel() {
    let f = fixture();
    keep(&f.engine, "a note");
    {
        let conn = rusqlite::Connection::open(f.dir.path().join("nocturne.db")).unwrap();
        conn.execute_batch(
            "INSERT INTO remote_devices(id, name, public_key, home_origin, connection, address, \
               paired_at) VALUES ('dev1', 'Pixel', \
               x'50414952494e4753454352455450414952494e4753454352455430303030', \
               'https://a.example', 'simple', 'a.example', '2026-11-13T07:00:00.000Z'); \
             INSERT INTO remote_keeps(item_id, device_id, client_id, kept_at) \
               SELECT id, 'dev1', 'c-1', kept_at FROM items LIMIT 1; \
             UPDATE remote_state SET wanted_on = 1, address = 'a.example', \
               first_on_at = '2026-11-13T07:00:00.000Z'; \
             UPDATE online_state SET wanted_on = 1, set_up = 1, \
               base_url = 'https://api.example.test/v1', key_storage = 'file', model = 'm'; \
             UPDATE reindex_state SET model = 'REINDEXMODELSECRET9', from_model = 'old', \
               total = 4, phase = 'running' WHERE id = 1; \
             INSERT INTO audio_cache(page_date, voice, path) \
               VALUES ('2026-11-14', 'af_heart', '/tmp/spoken.wav'); \
             INSERT INTO forget_tokens(token, ids, include_originals, stamp, expires_at) \
               VALUES ('tok', '[\"mem\"]', 0, 's', '2099-01-01T00:00:00Z');",
        )
        .unwrap();
    }

    let out = tempfile::tempdir().unwrap();
    let dest = backup(&f, out.path());
    let summary = check(&dest).unwrap();
    assert!(summary.online_used, "online answers was in use");
    assert!(summary.away_used, "a phone was paired");

    let (out, _) = unpack(&dest);
    let conn = rusqlite::Connection::open(out.path().join("stage/store/nocturne.db")).unwrap();
    let (key_len, revoked, address): (i64, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT length(public_key), revoked_at, address FROM remote_devices WHERE id = 'dev1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(key_len, 0, "the pairing secret is gone");
    assert!(revoked.is_some(), "the phone is marked as let go");
    assert!(address.is_none());
    let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(
        count("SELECT COUNT(*) FROM remote_state WHERE wanted_on = 0 AND address IS NULL"),
        1
    );
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM online_state WHERE wanted_on = 0 AND set_up = 0 \
             AND base_url IS NULL AND key_storage IS NULL"
        ),
        1
    );
    assert_eq!(count("SELECT COUNT(*) FROM audio_cache"), 0);
    assert_eq!(count("SELECT COUNT(*) FROM forget_tokens"), 0);

    let plain = payload(&dest, PASS);
    assert!(!contains(&plain, b"PAIRINGSECRET"));
    assert!(!contains(&plain, b"api.example.test"));
    assert!(!contains(&plain, b"REINDEXMODELSECRET9"));
}

#[test]
fn unpacking_scrubs_what_belongs_to_the_old_machine() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("nocturne.db");
    let db = {
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        for (version, sql) in nocturne_engine::store::migrations() {
            conn.execute_batch(sql).unwrap();
            conn.pragma_update(None, "user_version", version).unwrap();
        }
        conn.execute_batch(
            "INSERT INTO memories(id, subject, subject_key, predicate, object, object_key, \
               sentence, recorded_at, confidence, status, tags) VALUES \
               ('mem_x', 'Me', 'me', 'likes', 'tea', 'tea', 'You like tea.', \
                '2026-11-13T07:00:00.000Z', 0.9, 'active', '[]'); \
             UPDATE online_state SET set_up = 1, wanted_on = 1, \
               base_url = 'https://api.example.test/v1', key_storage = 'file' WHERE id = 1; \
             UPDATE remote_state SET wanted_on = 1, address = 'a.example', \
               first_on_at = '2026-11-13T07:00:00.000Z' WHERE id = 1; \
             INSERT INTO remote_devices(id, name, public_key, home_origin, connection, address, \
               paired_at) VALUES ('dev1', 'Pixel', x'50414952494e4753', 'https://a.example', \
               'simple', 'a.example', '2026-11-13T07:00:00.000Z'); \
             INSERT INTO forget_tokens(token, ids, include_originals, stamp, expires_at) \
               VALUES ('tok', '[\"mem_x\"]', 0, 's', '2099-01-01T00:00:00Z'); \
             INSERT INTO audio_cache(page_date, voice, path) \
               VALUES ('2026-11-14', 'af_heart', '/tmp/spoken.wav'); \
             UPDATE reindex_state SET model = 'other', from_model = 'old', total = 4, \
               phase = 'running' WHERE id = 1; \
             INSERT INTO embeddings_next(memory_id, model, dim, vec) \
               VALUES ('mem_x', 'other', 2, x'0000803f00000000'); \
             INSERT INTO forget_jobs(ids, items, include_originals) \
               VALUES ('[\"mem_x\"]', '[\"itm_x\"]', 1);",
        )
        .unwrap();
        drop(conn);
        std::fs::read(&db_path).unwrap()
    };
    let file = dir.path().join("old-machine");
    build(
        &file,
        &manifest_text(1, nocturne_engine::store::latest_version()),
        &db,
    );

    let (out, summary) = unpack(&file);
    assert_eq!(
        summary.store_version,
        nocturne_engine::store::latest_version()
    );
    let conn = rusqlite::Connection::open(out.path().join("stage/store/nocturne.db")).unwrap();
    let version: u32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, nocturne_engine::store::latest_version());
    let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM online_state WHERE wanted_on = 0 AND set_up = 0 \
             AND base_url IS NULL AND key_storage IS NULL"
        ),
        1,
        "online answers is off"
    );
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM remote_state WHERE wanted_on = 0 AND address IS NULL \
             AND first_on_at IS NULL"
        ),
        1,
        "Away from home is off"
    );
    assert_eq!(count("SELECT COUNT(*) FROM audio_cache"), 0);
    assert_eq!(count("SELECT COUNT(*) FROM forget_tokens"), 0);
    assert_eq!(count("SELECT COUNT(*) FROM embeddings_next"), 0);
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM reindex_state WHERE model IS NULL AND phase IS NULL \
             AND total = 0"
        ),
        1,
        "no half-finished model change"
    );
    let (key_len, revoked, address): (i64, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT length(public_key), revoked_at, address FROM remote_devices WHERE id = 'dev1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(key_len, 0, "the pairing secret is gone");
    assert!(revoked.is_some());
    assert!(address.is_none());
    assert_eq!(
        count("SELECT COUNT(*) FROM forget_jobs"),
        1,
        "a forget in progress must still finish"
    );
    assert_eq!(count("SELECT COUNT(*) FROM memories"), 1);
}

#[test]
fn an_original_in_a_pending_forget_is_left_out() {
    let f = fixture();
    let src = tempfile::tempdir().unwrap();
    let photo = match f
        .engine
        .keep_file(
            &write_file(src.path(), "secret.jpg", b"JPEG bytes of the secret card"),
            Source::Screenshot,
        )
        .unwrap()
    {
        Kept::New { item } => item.id,
        _ => unreachable!(),
    };
    f.engine
        .apply(
            &photo,
            vec![cand("Zed", Predicate::Knows, "x", "Zed knows x.")],
        )
        .unwrap();
    let memory = f.engine.memories(Default::default()).unwrap()[0].id.clone();
    let preview = f
        .engine
        .preview_forget(std::slice::from_ref(&memory), true)
        .unwrap();
    f.engine.forget_begin_only(&preview.token).unwrap();

    let out = tempfile::tempdir().unwrap();
    let dest = backup(&f, out.path());
    let summary = check(&dest).unwrap();
    assert_eq!(summary.originals, 0);
    assert_eq!(summary.items, 1, "the item row is still there");

    let (out, _) = unpack(&dest);
    assert_eq!(
        std::fs::read_dir(out.path().join("stage/store/originals"))
            .unwrap()
            .count(),
        0,
        "the original is not"
    );
    let conn = rusqlite::Connection::open(out.path().join("stage/store/nocturne.db")).unwrap();
    let kept: i64 = conn
        .query_row("SELECT COUNT(*) FROM items WHERE id = ?1", [&photo], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(kept, 1);
}

#[test]
fn a_short_passphrase_is_refused_and_the_same_characters_open_it() {
    let f = fixture();
    keep(&f.engine, "a note");
    let out = tempfile::tempdir().unwrap();
    let dest = out.path().join("chosen.nocturne");
    let cancel = no_cancel();
    let run = BackupRun {
        extras: &[],
        progress: &noop,
        cancel: &cancel,
        started: &|_| {},
        work_factor: Some(TEST_WORK),
    };
    let err = f.engine.backup_to(&dest, "1234567", &run).unwrap_err();
    assert!(matches!(err, Error::Invalid(_)), "{err:?}");
    assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0);
    assert!(!f.dir.path().join("backup-tmp").exists());

    let nfc = out.path().join("nfc.nocturne");
    let run = BackupRun {
        extras: &[],
        progress: &noop,
        cancel: &cancel,
        started: &|_| {},
        work_factor: Some(TEST_WORK),
    };
    f.engine
        .backup_to(&nfc, "caf\u{e9}-passphrase", &run)
        .unwrap();
    let scratch = out.path().join("scratch");
    let summary = check_backup(&nfc, "cafe\u{301}-passphrase", &scratch, &noop, &cancel).unwrap();
    assert_eq!(summary.items, 1);
}

#[test]
fn the_suggested_name_is_a_nocturne_file_for_today() {
    let f = fixture();
    assert_eq!(
        f.engine.suggested_backup_name(),
        "Nocturne backup 2026-11-13.nocturne"
    );
}

#[test]
fn holdings_says_what_is_here_and_what_came_after_the_backup() {
    let f = fixture();
    assert!(f.engine.holdings(None).unwrap().is_none());

    let src = tempfile::tempdir().unwrap();
    let original = vec![7u8; 4096];
    f.engine
        .keep_file(
            &write_file(src.path(), "card.jpg", &original),
            Source::Screenshot,
        )
        .unwrap();
    keep(&f.engine, "a plain note");
    f.clock.set_str("2026-11-14T06:00:00Z");
    let page = f
        .engine
        .compose_page(date("2026-11-14"), Edition::Screen)
        .unwrap();
    f.engine.save_page(&page).unwrap();

    let here = f.engine.holdings(None).unwrap().unwrap();
    assert_eq!((here.originals, here.pages), (1, 1));
    assert_eq!(here.kept_after_backup, 0);
    assert!(here.bytes >= 4096.0);
    let last = here.last_kept.clone().unwrap();

    f.clock.set_str("2026-11-15T07:00:00Z");
    keep(&f.engine, "kept later");
    let after = f.engine.holdings(Some(&last)).unwrap().unwrap();
    assert_eq!(after.kept_after_backup, 1);
    assert_eq!(after.originals, 1);
}

#[test]
fn the_estimate_covers_the_kept_originals() {
    let f = fixture();
    let src = tempfile::tempdir().unwrap();
    let original = vec![3u8; 50_000];
    f.engine
        .keep_file(
            &write_file(src.path(), "big.bin.txt", &original),
            Source::Paste,
        )
        .unwrap();
    let estimate = f.engine.backup_estimate().unwrap();
    assert!(estimate >= 50_000, "{estimate}");
    let store = std::fs::metadata(f.dir.path().join("nocturne.db"))
        .unwrap()
        .len();
    assert!(estimate >= store);
}

#[test]
fn has_data_says_whether_the_store_holds_anything_of_the_persons() {
    let f = fixture();
    assert!(!f.engine.has_data().unwrap());
    keep(&f.engine, "a note");
    assert!(f.engine.has_data().unwrap());

    let g = fixture();
    g.engine
        .update_settings(SettingsPatch {
            page_time: Some("06:45".into()),
            ..Default::default()
        })
        .unwrap();
    assert!(g.engine.has_data().unwrap());
}
