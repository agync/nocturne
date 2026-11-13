//! The whole product against the real private engine and the real models, nothing scripted.
//!
//! It runs the app's own commands over the mock runtime, with the engine from `vendor/lemonade`
//! (or `NOCTURNE_LEMOND`), and walks one seeded day: setup of the small mind, a day of kept notes
//! and files, reading, twelve questions and six held out, the overnight tidy and the morning
//! page, forgetting, deleting everything and quitting. Each step records what it measured; the
//! run ends with a table and writes `results.json` and `summary.md`.
//!
//! ```text
//! make e2e-real                     # downloads the small mind once into target/e2e-real
//! NOCTURNE_E2E_DIR=/path make e2e-real
//! ```
//!
//! Environment: `NOCTURNE_E2E_DIR` (scratch folder, default `target/e2e-real`),
//! `NOCTURNE_E2E_FRESH=1` (forget what was downloaded before), `NOCTURNE_E2E_TAG` (suffix for the
//! result files), `NOCTURNE_E2E_READ_SECS` and `NOCTURNE_E2E_SETUP_SECS` (time limits).

mod common;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::*;
use nocturne_app_lib::os::notify;
use nocturne_app_lib::runtime::{self, StartOptions};
use nocturne_app_lib::{AppState, configure};
use nocturne_contract::{AskDelta, AskPhase, EVENT_ASK};
use nocturne_engine::{DELETE_PHRASE, ManualClock};
use nocturne_intelligence::{FakeHost, SchedStep, TidyStep};
use serde_json::{Map, Value, json};
use tauri::async_runtime::block_on;
use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri::{Listener, WebviewWindowBuilder};

const DAY: &str = "2026-11-12";
const PAGE_DAY: &str = "2026-11-13";
const PRIVATE_WORDS: [&str; 4] = ["cholesterol", "blood test", "5.9", "recheck"];
const JARGON: [&str; 7] = [
    "model",
    "inference",
    "token",
    "npu",
    "gpu",
    "embedding",
    "graph",
];

// ---------------------------------------------------------------------------------------------
// The seeded day. Everything is made up; nothing here is a real person's data.

struct Note {
    key: &'static str,
    at: &'static str,
    text: &'static str,
}

const NOTES: &[Note] = &[
    Note {
        key: "dentist",
        at: "08:05",
        text: "Dentist appointment with Dr Okafor on Wednesday 18 November at 10:00.",
    },
    Note {
        key: "gran",
        at: "08:40",
        text: "Gran lives in Leeds. Her birthday is on 3 December.",
    },
    Note {
        key: "trip",
        at: "09:10",
        text: "Leo's school trip to the science museum is on Monday 23 November. He needs a packed lunch.",
    },
    Note {
        key: "priya",
        at: "09:55",
        text: "I promised Priya I would send her the budget spreadsheet by Friday 20 November.",
    },
    Note {
        key: "bakery",
        at: "10:30",
        text: "Bakery order for Saturday: two sourdough loaves and one seeded rye, collect before noon from Fenn Street Bakery.",
    },
    Note {
        key: "club1",
        at: "11:15",
        text: "Book club meets on the first Tuesday of the month at the library.",
    },
    Note {
        key: "dentist_moved",
        at: "12:00",
        text: "The dentist appointment has moved. Dr Okafor's receptionist called: it is now Thursday 19 November at 14:30, not Wednesday 18 November.",
    },
    Note {
        key: "blood",
        at: "12:40",
        text: "My blood test showed high cholesterol, 5.9. The doctor wants a recheck in March.",
    },
    Note {
        key: "gran_moved",
        at: "13:20",
        text: "Gran has moved from Leeds to York as of 1 November. She now lives in York.",
    },
    Note {
        key: "swim",
        at: "14:00",
        text: "Leo swims on Mondays with Coach Dee after school.",
    },
    Note {
        key: "paint",
        at: "14:45",
        text: "Decision: we will repaint the hallway in sage green, not grey.",
    },
    Note {
        key: "boiler",
        at: "15:30",
        text: "Boiler service is booked for Monday 30 November, between 8 and 12. The engineer is called Marek.",
    },
    Note {
        key: "club2",
        at: "16:10",
        text: "Sam says book club is actually on the second Wednesday of the month at the community hall.",
    },
    Note {
        key: "mot",
        at: "17:20",
        text: "The car MOT is due on 14 December. The garage is Hale Motors.",
    },
    Note {
        key: "cinema",
        at: "18:05",
        text: "I told Sam I would book the cinema tickets this weekend.",
    },
    Note {
        key: "wifi",
        at: "19:30",
        text: "The guest wifi network is called HillviewGuest.",
    },
];

const PHOTO_AT: &str = "21:00";
const PHOTO_LINES: [&str; 4] = [
    "CITY PARKING PERMIT",
    "PERMIT NUMBER 4471",
    "BAY C7 LEVEL 2",
    "VALID UNTIL 30 JUNE 2027",
];
const VOICE_AT: &str = "21:30";
const VOICE_TEXT: &str = "Reminder to myself. The plumber, Mister Hughes, is coming on Friday the twentieth of November at nine in the morning to fix the kitchen tap.";
const DOC_AT: &str = "21:45";
const DOC_LINES: [&str; 4] = [
    "Riverside Allotment Society, autumn newsletter",
    "Our plot is number 17, next to the water tap.",
    "The water is switched off for winter on 1 December.",
    "The annual fee of 30 pounds is due by 15 January.",
];

struct Question {
    id: &'static str,
    kind: &'static str,
    text: &'static str,
    /// The kept things the answer must come from.
    sources: &'static [&'static str],
    /// Every group needs one of its words in the answer.
    must: &'static [&'static [&'static str]],
    /// The right answer is to say it does not know.
    unknown: bool,
}

const QUESTIONS: &[Question] = &[
    Question {
        id: "q01",
        kind: "fact",
        text: "Who is coming to service the boiler, and when?",
        sources: &["boiler"],
        must: &[&["marek"], &["30 november", "november 30", "30th"]],
        unknown: false,
    },
    Question {
        id: "q02",
        kind: "changed fact",
        text: "When is my dentist appointment?",
        sources: &["dentist_moved", "dentist"],
        must: &[&["19 november", "thursday"], &["14:30", "2:30", "14.30"]],
        unknown: false,
    },
    Question {
        id: "q03",
        kind: "promise",
        text: "What did I promise Priya?",
        sources: &["priya"],
        must: &[&["budget"], &["friday", "20 november"]],
        unknown: false,
    },
    Question {
        id: "q04",
        kind: "photo only",
        text: "What is my parking permit number?",
        sources: &["photo"],
        must: &[&["4471"]],
        unknown: false,
    },
    Question {
        id: "q05",
        kind: "voice only",
        text: "When is the plumber coming?",
        sources: &["voice", "voice_raw"],
        must: &[&["friday", "20", "twentieth"], &["9", "nine"]],
        unknown: false,
    },
    Question {
        id: "q06",
        kind: "private, asked directly",
        text: "What did my blood test show?",
        sources: &["blood"],
        must: &[&["cholesterol"], &["5.9"]],
        unknown: false,
    },
    Question {
        id: "q07",
        kind: "never kept",
        text: "What is Sam's mother's phone number?",
        sources: &[],
        must: &[],
        unknown: true,
    },
    Question {
        id: "q08",
        kind: "which is right",
        text: "When does book club meet?",
        sources: &["club1", "club2"],
        must: &[&["tuesday"], &["wednesday"]],
        unknown: false,
    },
    Question {
        id: "q09",
        kind: "changed fact",
        text: "Where does Gran live?",
        sources: &["gran_moved", "gran"],
        must: &[&["york"]],
        unknown: false,
    },
    Question {
        id: "q10",
        kind: "document only",
        text: "What is our allotment plot number?",
        sources: &["doc"],
        must: &[&["17"]],
        unknown: false,
    },
    Question {
        id: "q11",
        kind: "decision",
        text: "What colour are we painting the hallway?",
        sources: &["paint"],
        must: &[&["sage"]],
        unknown: false,
    },
    Question {
        id: "q12",
        kind: "second promise",
        text: "What did I tell Sam I would book this weekend?",
        sources: &["cinema"],
        must: &[&["cinema"]],
        unknown: false,
    },
];

// Held out: six questions and their notes, written before the extraction prompt was changed and
// never used to tune it. They are reported on their own, next to the twelve above.

const HELD_OUT_NOTES: &[Note] = &[
    Note {
        key: "ho_vet",
        at: "08:20",
        text: "The vet, Dr Amara Lind, says Biscuit needs his booster jab on Tuesday 1 December at 16:15.",
    },
    Note {
        key: "ho_locker",
        at: "10:50",
        text: "Gym locker code is 2958. My locker is number 44 in the east changing room.",
    },
    Note {
        key: "ho_nadia",
        at: "13:40",
        text: "Nadia lent me her blue camping stove. I said I would give it back after the half-term trip.",
    },
    Note {
        key: "ho_train",
        at: "15:05",
        text: "Booked the 07:42 train to Bristol Temple Meads for Friday 27 November, coach D, seat 31.",
    },
    Note {
        key: "ho_tom",
        at: "17:45",
        text: "Tom started a new job at Kestrel Labs this week. He works on their water sensor project.",
    },
    Note {
        key: "ho_bins",
        at: "20:15",
        text: "Recycling bins go out every other Tuesday; the next collection is on 24 November.",
    },
];

const HELD_OUT: &[Question] = &[
    Question {
        id: "h01",
        kind: "appointment",
        text: "When is Biscuit's booster jab?",
        sources: &["ho_vet"],
        must: &[
            &["1 december", "december 1", "tuesday"],
            &["16:15", "4:15", "16.15"],
        ],
        unknown: false,
    },
    Question {
        id: "h02",
        kind: "number among numbers",
        text: "What is my gym locker code?",
        sources: &["ho_locker"],
        must: &[&["2958"]],
        unknown: false,
    },
    Question {
        id: "h03",
        kind: "borrowed thing",
        text: "What did Nadia lend me?",
        sources: &["ho_nadia"],
        must: &[&["stove"]],
        unknown: false,
    },
    Question {
        id: "h04",
        kind: "detail among details",
        text: "Which seat do I have on the Bristol train?",
        sources: &["ho_train"],
        must: &[&["31"]],
        unknown: false,
    },
    Question {
        id: "h05",
        kind: "pronoun subject",
        text: "Where does Tom work now?",
        sources: &["ho_tom"],
        must: &[&["kestrel"]],
        unknown: false,
    },
    Question {
        id: "h06",
        kind: "next date",
        text: "When is the next recycling collection?",
        sources: &["ho_bins"],
        must: &[&["24 november", "november 24", "24th"]],
        unknown: false,
    },
];

/// What to forget: the first of these that has a memory by then.
struct ForgetTarget {
    key: &'static str,
    question: &'static str,
    words: &'static [&'static str],
}

const FORGET_TARGETS: &[ForgetTarget] = &[
    ForgetTarget {
        key: "bakery",
        question: "What did I order from the bakery?",
        words: &["sourdough", "rye", "fenn"],
    },
    ForgetTarget {
        key: "cinema",
        question: "What did I tell Sam I would book this weekend?",
        words: &["cinema"],
    },
    ForgetTarget {
        key: "priya",
        question: "What did I promise Priya?",
        words: &["budget", "spreadsheet"],
    },
];

// ---------------------------------------------------------------------------------------------
// Making a photo and a document at test time, with no tools and no files in the repository.

#[rustfmt::skip]
const GLYPHS: &[(char, [u8; 7])] = &[
    (
        'A',
        [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
    ),
    (
        'B',
        [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
    ),
    (
        'C',
        [
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
    ),
    (
        'D',
        [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
    ),
    (
        'E',
        [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
    ),
    (
        'F',
        [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
    ),
    (
        'G',
        [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ],
    ),
    (
        'H',
        [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
    ),
    (
        'I',
        [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
    ),
    (
        'J',
        [
            0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100,
        ],
    ),
    (
        'K',
        [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
    ),
    (
        'L',
        [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
    ),
    (
        'M',
        [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
    ),
    (
        'N',
        [
            0b10001, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001,
        ],
    ),
    (
        'O',
        [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        'P',
        [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
    ),
    (
        'Q',
        [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
    ),
    (
        'R',
        [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
    ),
    (
        'S',
        [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
    ),
    (
        'T',
        [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
    ),
    (
        'U',
        [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        'V',
        [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
    ),
    (
        'W',
        [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
    ),
    (
        'X',
        [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
    ),
    (
        'Y',
        [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
    ),
    (
        'Z',
        [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
    ),
    (
        '0',
        [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
    ),
    (
        '1',
        [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
    ),
    (
        '2',
        [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
    ),
    (
        '3',
        [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
    ),
    (
        '4',
        [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
    ),
    (
        '5',
        [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
    ),
    (
        '6',
        [
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        '7',
        [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
    ),
    (
        '8',
        [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
    ),
    (
        '9',
        [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
        ],
    ),
];

fn crc32(bytes: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in bytes {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + u32::from(x)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
    out.extend((body.len() as u32).to_be_bytes());
    let mut covered = kind.to_vec();
    covered.extend(body);
    out.extend(&covered);
    out.extend(crc32(&covered).to_be_bytes());
}

/// A grey-scale PNG of upper-case lines in a 5 by 7 face, black on white, stored uncompressed.
fn png_of_text(lines: &[&str], scale: usize) -> Vec<u8> {
    let cell = 6 * scale;
    let width = lines.iter().map(|l| l.len()).max().unwrap_or(1) * cell + 4 * cell;
    let line_height = 10 * scale;
    let height = lines.len() * line_height + 2 * line_height;
    let mut pixels = vec![255u8; width * height];
    for (row, line) in lines.iter().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            let Some((_, glyph)) = GLYPHS.iter().find(|(c, _)| *c == ch) else {
                continue;
            };
            for (gy, bits) in glyph.iter().enumerate() {
                for gx in 0..5 {
                    if bits >> (4 - gx) & 1 == 0 {
                        continue;
                    }
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let x = 2 * cell + col * cell + gx * scale + dx;
                            let y = line_height + row * line_height + gy * scale + dy;
                            pixels[y * width + x] = 0;
                        }
                    }
                }
            }
        }
    }
    let mut raw = Vec::with_capacity((width + 1) * height);
    for row in pixels.chunks(width) {
        raw.push(0);
        raw.extend(row);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (i, block) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        z.extend((block.len() as u16).to_le_bytes());
        z.extend((!(block.len() as u16)).to_le_bytes());
        z.extend(*block);
    }
    z.extend(adler32(&raw).to_be_bytes());
    let mut ihdr = Vec::new();
    ihdr.extend((width as u32).to_be_bytes());
    ihdr.extend((height as u32).to_be_bytes());
    ihdr.extend([8, 0, 0, 0, 0]);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png_chunk(&mut png, b"IHDR", &ihdr);
    png_chunk(&mut png, b"IDAT", &z);
    png_chunk(&mut png, b"IEND", &[]);
    png
}

/// A one-page PDF with the lines in Helvetica. The lines hold no parentheses.
fn pdf_of_text(lines: &[&str]) -> Vec<u8> {
    let mut content = String::from("BT /F1 14 Tf 72 720 Td 22 TL\n");
    for line in lines {
        content.push_str(&format!("({line}) Tj T*\n"));
    }
    content.push_str("ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R \
         /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_string(),
        format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_string(),
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend(format!("{} 0 obj\n{body}\nendobj\n", i + 1).bytes());
    }
    let xref = pdf.len();
    pdf.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
    for off in offsets {
        pdf.extend(format!("{off:010} 00000 n \n").bytes());
    }
    pdf.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .bytes(),
    );
    pdf
}

/// The same WAV with its sizes written in, for engines that stream a file without knowing its
/// length (the voice does: both sizes read 0xFFFFFFFF).
fn wav_with_sizes(wav: &[u8]) -> Vec<u8> {
    let mut out = wav.to_vec();
    let Some(at) = out.windows(4).position(|w| w == b"data") else {
        return out;
    };
    let body = out.len() - (at + 8);
    let riff = (out.len() - 8) as u32;
    out[4..8].copy_from_slice(&riff.to_le_bytes());
    out[at + 4..at + 8].copy_from_slice(&(body as u32).to_le_bytes());
    out
}

// ---------------------------------------------------------------------------------------------
// The run: checks, measurements and the files they end up in.

struct Check {
    id: &'static str,
    what: String,
    pass: bool,
    detail: String,
}

struct Run {
    dir: PathBuf,
    tag: String,
    started: Instant,
    checks: Vec<Check>,
    out: Map<String, Value>,
}

impl Run {
    fn put(&mut self, key: &str, value: Value) {
        self.out.insert(key.to_string(), value);
        self.save();
    }

    fn check(&mut self, id: &'static str, what: &str, pass: bool, detail: impl Into<String>) {
        let detail = detail.into();
        println!(
            "[{}] {id}: {what}{}",
            if pass { "pass" } else { "FAIL" },
            if detail.is_empty() {
                String::new()
            } else {
                format!(" ({detail})")
            }
        );
        self.checks.push(Check {
            id,
            what: what.to_string(),
            pass,
            detail,
        });
        self.save();
    }

    fn save(&self) {
        let mut all = self.out.clone();
        all.insert(
            "checks".into(),
            Value::Array(
                self.checks
                    .iter()
                    .map(
                        |c| json!({"id": c.id, "what": c.what, "pass": c.pass, "detail": c.detail}),
                    )
                    .collect(),
            ),
        );
        all.insert(
            "elapsed_seconds".into(),
            json!(self.started.elapsed().as_secs()),
        );
        let _ = std::fs::create_dir_all(&self.dir);
        let _ = std::fs::write(
            self.dir.join(format!("results{}.json", self.tag)),
            serde_json::to_vec_pretty(&Value::Object(all)).unwrap_or_default(),
        );
    }

    fn fatal(&mut self, what: &str) -> ! {
        self.check("fatal", what, false, "");
        panic!("{what}");
    }
}

fn wait_until(limit: Duration, mut done: impl FnMut() -> bool) -> Option<Duration> {
    let start = Instant::now();
    loop {
        if done() {
            return Some(start.elapsed());
        }
        if start.elapsed() > limit {
            return None;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn env_secs(name: &str, default: u64) -> Duration {
    Duration::from_secs(
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default),
    )
}

fn ms(d: Duration) -> u64 {
    d.as_millis() as u64
}

fn percentile(values: &[u64], p: f64) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = ((p / 100.0) * sorted.len() as f64).ceil().max(1.0) as usize;
    Some(sorted[rank.min(sorted.len()) - 1])
}

fn dir_bytes(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_bytes(&e.path()),
            Ok(m) if m.is_file() => m.len(),
            _ => 0,
        })
        .sum()
}

/// Process ids in a process group, read from `/proc`.
fn group_members(pgid: u32) -> Vec<u32> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return found;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        // The name is in parentheses and may hold spaces; the fields follow the last one.
        let Some(rest) = stat.rsplit_once(')').map(|(_, r)| r) else {
            continue;
        };
        let fields: Vec<&str> = rest.split_whitespace().collect();
        if fields.get(2).and_then(|f| f.parse::<u32>().ok()) == Some(pgid) {
            found.push(pid);
        }
    }
    found
}

/// Process ids whose command line mentions `needle`.
fn processes_mentioning(needle: &str) -> Vec<u32> {
    let mut found = Vec::new();
    let me = std::process::id();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return found;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == me {
            continue;
        }
        if let Ok(cmd) = std::fs::read(entry.path().join("cmdline"))
            && String::from_utf8_lossy(&cmd).contains(needle)
        {
            found.push(pid);
        }
    }
    found
}

// ---------------------------------------------------------------------------------------------
// The app under test.

fn build_app(data_dir: &Path) -> Harness {
    let clock = Arc::new(ManualClock::at(
        &format!("{DAY}T07:30:00Z"),
        "Europe/London",
    ));
    let state = AppState::open(data_dir, clock.clone());
    let app = configure(mock_builder().manage(state))
        .build(mock_context(noop_assets()))
        .expect("the app builds");
    nocturne_app_lib::allow_originals_in_the_interface(app.handle());
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("a window");
    Harness {
        app,
        webview,
        // Unused: the data folder is the run's own, so that what was downloaded stays.
        dir: tempfile::tempdir().expect("a scratch folder"),
        clock,
    }
}

/// Start over with the store and the logs; keep what the engine downloaded unless asked.
fn prepare_data_dir(data_dir: &Path) {
    if std::env::var("NOCTURNE_E2E_FRESH").as_deref() == Ok("1") {
        let _ = std::fs::remove_dir_all(data_dir);
    } else if let Ok(entries) = std::fs::read_dir(data_dir) {
        for e in entries.flatten() {
            if e.file_name() != "engine" {
                let _ = std::fs::remove_dir_all(e.path());
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    std::fs::create_dir_all(data_dir).expect("the data folder");
}

fn at(h: &Harness, day: &str, hm: &str) {
    h.clock.set_str(&format!("{day}T{hm}:00Z"));
}

struct Asker {
    deltas: Arc<Mutex<Vec<(Instant, AskDelta)>>>,
}

#[derive(Default, Clone)]
struct Answer {
    phase: String,
    text: String,
    first_delta_ms: Option<u64>,
    first_sentence_ms: Option<u64>,
    total_ms: u64,
    saw_waking: bool,
    found: Option<u32>,
    memory_ids: Vec<String>,
    receipts: Vec<Value>,
    near: Vec<Value>,
    /// The engine dropped the bold: it could not be matched one for one to the receipts.
    plain: bool,
}

impl Asker {
    fn new(h: &Harness) -> Asker {
        let deltas: Arc<Mutex<Vec<(Instant, AskDelta)>>> = Arc::default();
        let sink = deltas.clone();
        h.app.listen(EVENT_ASK, move |ev| {
            if let Ok(d) = serde_json::from_str::<AskDelta>(ev.payload()) {
                sink.lock().unwrap().push((Instant::now(), d));
            }
        });
        Asker { deltas }
    }

    fn ask(&self, h: &Harness, question: &str) -> Answer {
        let t0 = Instant::now();
        let id = h
            .ok("ask", json!({ "question": question }))
            .as_str()
            .unwrap()
            .to_string();
        let mut answer = Answer {
            phase: "timeout".into(),
            ..Answer::default()
        };
        let mut seen = 0;
        let limit = Duration::from_secs(300);
        while t0.elapsed() < limit {
            let mine: Vec<(Instant, AskDelta)> = self
                .deltas
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, d)| d.ask_id == id)
                .cloned()
                .collect();
            for (when, d) in mine.iter().skip(seen) {
                let since = ms(when.duration_since(t0));
                match d.phase {
                    AskPhase::Waking => answer.saw_waking = true,
                    AskPhase::Reading => answer.found = d.found,
                    AskPhase::Streaming => {
                        if let Some(t) = &d.text_delta {
                            answer.first_delta_ms.get_or_insert(since);
                            answer.text.push_str(t);
                            if answer.first_sentence_ms.is_none()
                                && answer.text.contains(['.', '!', '?'])
                            {
                                answer.first_sentence_ms = Some(since);
                            }
                        }
                    }
                    _ => {}
                }
                if is_terminal(d.phase) {
                    answer.phase = serde_json::to_value(d.phase)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_string();
                    answer.total_ms = since;
                    answer.memory_ids = d.memory_ids.clone();
                    if d.plain {
                        answer.plain = true;
                        answer.text = plain(&answer.text);
                    }
                    answer.receipts = d
                        .receipts
                        .iter()
                        .map(|r| serde_json::to_value(r).unwrap())
                        .collect();
                    answer.near = d
                        .near
                        .iter()
                        .map(|n| serde_json::to_value(n).unwrap())
                        .collect();
                    if answer.first_sentence_ms.is_none() {
                        answer.first_sentence_ms = answer.first_delta_ms.or(Some(since));
                    }
                    return answer;
                }
            }
            seen = mine.len();
            std::thread::sleep(Duration::from_millis(20));
        }
        h.ok("cancel_ask", json!({ "askId": id }));
        answer.total_ms = ms(t0.elapsed());
        answer
    }
}

fn plain(text: &str) -> String {
    text.replace("**", "")
}

fn bold_spans(text: &str) -> Vec<String> {
    text.split("**")
        .enumerate()
        .filter(|(i, s)| i % 2 == 1 && !s.is_empty())
        .map(|(_, s)| s.to_string())
        .collect()
}

fn words_of(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

fn answer_json(q: &Question, a: &Answer, extra: Value) -> Value {
    json!({
        "id": q.id, "kind": q.kind, "question": q.text, "phase": a.phase,
        "text": plain(&a.text), "raw_text": a.text,
        "first_delta_ms": a.first_delta_ms, "first_sentence_ms": a.first_sentence_ms,
        "total_ms": a.total_ms, "saw_waking": a.saw_waking, "found": a.found,
        "memory_ids": a.memory_ids, "receipts": a.receipts, "near": a.near, "bold_dropped": a.plain,
        "judged": extra,
    })
}

#[test]
#[ignore = "downloads models; run with make e2e-real"]
fn the_whole_product_works_with_real_models() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let scratch = std::env::var_os("NOCTURNE_E2E_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../target/e2e-real"));
    std::fs::create_dir_all(&scratch).expect("the scratch folder");
    let scratch = scratch.canonicalize().unwrap();
    let data_dir = scratch.join("data");
    prepare_data_dir(&data_dir);
    let tag = std::env::var("NOCTURNE_E2E_TAG").unwrap_or_default();
    let mut run = Run {
        dir: scratch.join("results"),
        tag,
        started: Instant::now(),
        checks: Vec::new(),
        out: Map::new(),
    };
    println!("scratch folder: {}", scratch.display());
    if let Ok(file) = std::fs::File::create(run.dir.join(format!("app{}.log", run.tag))) {
        let _ = tracing_subscriber::fmt()
            .with_writer(Mutex::new(file))
            .with_ansi(false)
            .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
            .try_init();
    }

    let h = build_app(&data_dir);
    let asks = Asker::new(&h);
    let host = FakeHost::new();
    let bundle = runtime::lemonade::find_bundle(h.app.handle())
        .unwrap_or_else(|| panic!("no engine bundle: run scripts/fetch-lemonade.sh"));
    let services = start(&h, StartOptions::with_engine(host.clone(), bundle.clone()));
    let engine_root = data_dir.join("engine");

    let ram_gib = total_memory_gib();
    let load_at_start = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    run.put(
        "meta",
        json!({
            "day": DAY, "page_day": PAGE_DAY, "cpus": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
            "ram_gib": ram_gib, "loadavg_at_start": load_at_start.trim(), "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
            "note": "a tiny sample of 12 questions, not the 100-question benchmark in docs/design/evaluation.md",
        }),
    );

    // 1. Setup -----------------------------------------------------------------------------
    let reached = wait_until(Duration::from_secs(180), || {
        matches!(
            h.ok("get_setup", json!({}))["phase"].as_str(),
            Some("choosing" | "done" | "failed")
        )
    });
    if reached.is_none() {
        run.fatal("setup never reached a choice or done");
    }
    let first = h.ok("get_setup", json!({}));
    let was_done = first["phase"] == "done";
    run.put("setup_first_view", first.clone());
    let mut phases_seen: Vec<String> = vec![first["phase"].as_str().unwrap().to_string()];
    let mut setup_ms = 0;
    let mut setup_bytes = 0.0;
    let mut offline_during_download: Vec<bool> = Vec::new();
    if !was_done {
        let small = first["options"]
            .as_array()
            .and_then(|o| o.iter().find(|o| o["tier"] == "small").cloned());
        run.check(
            "setup.choice",
            "the small tier is offered and fits this machine",
            small.as_ref().is_some_and(|s| s["fits"] == true),
            small.map(|s| s.to_string()).unwrap_or_default(),
        );
        let t0 = Instant::now();
        h.ok("start_setup", json!({"tier": "small"}));
        let limit = env_secs("NOCTURNE_E2E_SETUP_SECS", 4 * 3600);
        let mut last_print = Instant::now();
        let done = wait_until(limit, || {
            let v = h.ok("get_setup", json!({}));
            let phase = v["phase"].as_str().unwrap_or("").to_string();
            if phases_seen.last() != Some(&phase) {
                phases_seen.push(phase.clone());
            }
            setup_bytes = v["downloadedBytes"].as_f64().unwrap_or(setup_bytes);
            if phase == "downloading" {
                let off = block_on(async {
                    let client = services.client().await?;
                    client.config().await.ok()?["offline"].as_bool()
                });
                if let Some(o) = off
                    && offline_during_download.len() < 5
                {
                    offline_during_download.push(o);
                }
            }
            if last_print.elapsed() > Duration::from_secs(30) {
                last_print = Instant::now();
                println!(
                    "setup {phase}: {:.2} of {:.2} GB",
                    v["downloadedBytes"].as_f64().unwrap_or(0.0) / 1e9,
                    v["totalBytes"].as_f64().unwrap_or(0.0) / 1e9
                );
            }
            matches!(phase.as_str(), "done" | "failed")
        });
        setup_ms = ms(t0.elapsed());
        let last = h.ok("get_setup", json!({}));
        run.check(
            "setup.done",
            "setup downloads the small tier and reaches done",
            done.is_some() && last["phase"] == "done",
            format!(
                "{:?} in {}s, error {}",
                phases_seen,
                setup_ms / 1000,
                last["error"]
            ),
        );
        if last["phase"] != "done" {
            run.fatal("setup did not finish");
        }
    } else {
        run.check(
            "setup.done",
            "setup was already done in this scratch folder (models reused)",
            true,
            "",
        );
    }
    let small_bytes = nocturne_lemonade::tier::download_bytes(nocturne_contract::Tier::Small);
    run.put(
        "setup",
        json!({
            "downloaded_this_run": !was_done, "phases": phases_seen, "wall_ms": setup_ms,
            "downloaded_bytes": setup_bytes, "tier_download_bytes": small_bytes,
            "engine_offline_samples_while_downloading": offline_during_download,
        }),
    );

    let mind_in = wait_until(Duration::from_secs(600), || h.state().mind.get().is_some());
    run.check(
        "setup.mind",
        "the mind arrives in the slot after setup",
        mind_in.is_some(),
        mind_in
            .map(|d| format!("{}s", d.as_secs()))
            .unwrap_or_default(),
    );
    if mind_in.is_none() {
        run.fatal("no mind");
    }
    let engine_pid = block_on(services.engine_pid());
    let offline_after = wait_until(Duration::from_secs(120), || {
        block_on(async {
            let Some(client) = services.client().await else {
                return false;
            };
            client
                .config()
                .await
                .map(|c| c["offline"] == true && c["no_fetch_executables"] == true)
                .unwrap_or(false)
        })
    });
    run.check(
        "setup.offline",
        "the engine is offline again after setup (offline and no executable fetching)",
        offline_after.is_some(),
        "",
    );
    let models_bytes_after_setup = dir_bytes(&engine_root.join("models"));
    let ledger_after_setup = h.ok("get_privacy", json!({}))["ledger"]
        .as_array()
        .map(Vec::len)
        .unwrap_or(0);
    let advanced = h.ok("get_advanced", json!({}));
    run.check(
        "setup.advanced",
        "the advanced details name the tier, the components and the engine version",
        advanced["tier"] == "small"
            && advanced["components"]
                .as_array()
                .is_some_and(|c| !c.is_empty())
            && advanced["engineVersion"].is_string(),
        format!(
            "tier {} components {} engine {}",
            advanced["tier"],
            advanced["components"].as_array().map(Vec::len).unwrap_or(0),
            advanced["engineVersion"]
        ),
    );
    run.put(
        "advanced_after_setup",
        json!({
            "tier": advanced["tier"], "gpu": advanced["gpu"], "npu": advanced["npu"],
            "engine_version": advanced["engineVersion"], "components": advanced["components"],
            "ledger_entries": ledger_after_setup, "models_bytes": models_bytes_after_setup,
        }),
    );

    // 2. Keep ------------------------------------------------------------------------------
    let mut ids: HashMap<&'static str, String> = HashMap::new();
    let keep_started = Instant::now();
    for note in NOTES.iter().chain(HELD_OUT_NOTES) {
        at(&h, DAY, note.at);
        let kept = h.ok(
            "keep_text",
            json!({"text": note.text, "source": "quickCapture"}),
        );
        ids.insert(note.key, id_of(&kept));
        if note.key == "blood" {
            h.ok(
                "set_item_private",
                json!({"id": ids["blood"], "private": true}),
            );
        }
    }
    at(&h, DAY, PHOTO_AT);
    let png = png_of_text(&PHOTO_LINES, 5);
    let kept = h.call_raw(
        "keep_bytes",
        png.clone(),
        &[
            ("x-nocturne-mime", "image/png"),
            ("x-nocturne-source", "screenshot"),
        ],
    );
    ids.insert("photo", id_of(&kept.expect("the photo is kept")));
    let _ = std::fs::write(scratch.join("photo.png"), &png);

    at(&h, DAY, VOICE_AT);
    let mind = h.state().mind.get().expect("a mind");
    let voice = h.ok("get_settings", json!({}))["voice"]
        .as_str()
        .unwrap()
        .to_string();
    let t_speak = Instant::now();
    let wav = block_on(mind.speak(VOICE_TEXT, &voice));
    let speak_ms = ms(t_speak.elapsed());
    let wav = match wav {
        Ok(w) if w.starts_with(b"RIFF") => w,
        other => {
            run.check(
                "keep.voice",
                "the bundled voice speaks the voice note",
                false,
                format!("{:?}", other.map(|w| w.len())),
            );
            run.fatal("no voice note");
        }
    };
    let _ = std::fs::write(scratch.join("voice.wav"), &wav);
    run.check(
        "keep.voice",
        "the bundled voice speaks the voice note",
        true,
        format!("{} KB in {speak_ms} ms", wav.len() / 1024),
    );
    // As the voice wrote it (sizes unknown), and with the sizes filled in.
    let kept = h.call_raw(
        "keep_recording",
        wav.clone(),
        &[("x-nocturne-mime", "audio/wav")],
    );
    ids.insert("voice_raw", id_of(&kept.expect("the recording is kept")));
    let kept = h.call_raw(
        "keep_recording",
        wav_with_sizes(&wav),
        &[("x-nocturne-mime", "audio/wav")],
    );
    ids.insert("voice", id_of(&kept.expect("the recording is kept")));

    at(&h, DAY, DOC_AT);
    let pdf = pdf_of_text(&DOC_LINES);
    let kept = h.call_raw(
        "keep_bytes",
        pdf,
        &[
            ("x-nocturne-mime", "application/pdf"),
            ("x-nocturne-source", "drop"),
        ],
    );
    ids.insert("doc", id_of(&kept.expect("the document is kept")));
    let by_id: HashMap<String, &'static str> = ids.iter().map(|(k, v)| (v.clone(), *k)).collect();
    run.check(
        "keep.all",
        "every note, the photo, the voice note (twice) and the document are kept",
        ids.len() == NOTES.len() + HELD_OUT_NOTES.len() + 4,
        format!("{} items in {} ms", ids.len(), ms(keep_started.elapsed())),
    );

    // 3. Reading ---------------------------------------------------------------------------
    at(&h, DAY, "22:00");
    let reading_started = Instant::now();
    let mut settled: BTreeMap<String, u64> = BTreeMap::new();
    let limit = env_secs("NOCTURNE_E2E_READ_SECS", 3600);
    let drained = wait_until(limit, || {
        let items = h.ok("list_items", json!({"filter": {}}));
        for item in items.as_array().unwrap() {
            let status = item["status"].as_str().unwrap_or("");
            if !matches!(status, "kept" | "reading") {
                settled
                    .entry(item["id"].as_str().unwrap().to_string())
                    .or_insert_with(|| ms(reading_started.elapsed()));
            }
        }
        ids.values().all(|id| settled.contains_key(id))
    });
    let reading_ms = ms(reading_started.elapsed());
    let mut item_rows = Vec::new();
    let mut previous = 0;
    let mut order: Vec<(&String, &u64)> = settled.iter().collect();
    order.sort_by_key(|(_, t)| **t);
    let mut couldnt = 0;
    for (id, at_ms) in order {
        let item = h.ok("get_item", json!({"id": id}));
        let memories = h
            .ok(
                "list_memories",
                json!({"filter": {"itemId": id, "limit": 100}}),
            )
            .as_array()
            .map(Vec::len)
            .unwrap_or(0);
        if item["status"] == "couldntRead" && by_id.get(id) != Some(&"voice_raw") {
            couldnt += 1;
        }
        item_rows.push(json!({
            "key": by_id.get(id).copied().unwrap_or("?"), "kind": item["kind"],
            "status": item["status"], "reason": item["reason"], "memories": memories,
            "engine_understood_in_ms": item["understoodInMs"],
            "settled_at_ms": at_ms, "since_previous_ms": at_ms.saturating_sub(previous),
            "transcript_or_text": item["text"], "summary": item["summary"],
            "private": item["private"],
        }));
        previous = *at_ms;
    }
    run.check(
        "read.all",
        "the reader understands every item within the time limit",
        drained.is_some() && couldnt == 0,
        format!("{} s, {couldnt} could not be read", reading_ms / 1000),
    );
    let untimed = item_rows
        .iter()
        .filter(|r| r["status"] == "understood" && r["engine_understood_in_ms"].is_null())
        .count();
    run.check(
        "read.timing",
        "the engine records how long each understood item took (understoodInMs)",
        untimed == 0,
        format!("{untimed} understood items have no time recorded"),
    );
    let raw_voice = item_rows.iter().find(|r| r["key"] == "voice_raw");
    run.check(
        "read.streamed_wav",
        "a WAV written by the bundled voice (sizes unknown in its header) can be read",
        raw_voice.is_some_and(|r| r["status"] != "couldntRead"),
        raw_voice
            .map(|r| format!("{} {}", r["status"], r["reason"]))
            .unwrap_or_default(),
    );
    let none_made = item_rows
        .iter()
        .filter(|r| r["memories"] == 0 && r["key"] != "voice_raw")
        .map(|r| r["key"].as_str().unwrap_or("?").to_string())
        .collect::<Vec<_>>();
    run.check(
        "read.memories",
        "every item yields at least one memory",
        none_made.is_empty(),
        format!("no memory from: {none_made:?}"),
    );
    let all_memories = h.ok("list_memories", json!({"filter": {"limit": 500}}));
    run.put(
        "reading",
        json!({
            "wall_ms": reading_ms, "items": item_rows,
            "memories": all_memories.as_array().map(|m| m.iter().map(|m| json!({
                "sentence": m["sentence"], "subject": m["subject"], "predicate": m["predicate"],
                "object": m["object"], "status": m["status"], "private": m["private"],
                "was": m["was"], "due": m["due"], "from": m["receipts"].as_array().map(|r| r.iter()
                    .filter_map(|r| r["itemId"].as_str().and_then(|i| by_id.get(i)).copied())
                    .collect::<Vec<_>>()),
            })).collect::<Vec<_>>()),
        }),
    );
    let blood_private = h.ok("get_item", json!({"id": ids["blood"]}))["private"] == true;
    let blood_memories = h.ok(
        "list_memories",
        json!({"filter": {"itemId": ids["blood"], "limit": 50}}),
    );
    run.check(
        "read.private",
        "memories read from the private note are private",
        blood_private
            && blood_memories
                .as_array()
                .is_some_and(|m| !m.is_empty() && m.iter().all(|m| m["private"] == true)),
        "",
    );
    let dentist = h.ok(
        "list_memories",
        json!({"filter": {"query": "dentist", "limit": 20}}),
    );
    let needs = h.ok("list_needs_you", json!({}));
    run.put(
        "dentist_state",
        json!({
            "memories": dentist.as_array().map(|m| m.iter().map(|m| json!({
                "sentence": m["sentence"], "status": m["status"], "was": m["was"],
                "due": m["due"], "validFrom": m["validFrom"], "validTo": m["validTo"],
            })).collect::<Vec<_>>()),
            "needs_you_entry_types": needs.as_array().map(|n| n.iter().map(|e| e["type"].clone()).collect::<Vec<_>>()),
        }),
    );

    // 4. Ask -------------------------------------------------------------------------------
    at(&h, DAY, "22:30");
    // The twelve, then the held-out six, each counted on its own.
    let mut rows: [Vec<Value>; 2] = Default::default();
    let mut tally = [Tally::default(), Tally::default()];
    let mut every_terminal = true;
    let asked = QUESTIONS
        .iter()
        .map(|q| (q, 0))
        .chain(HELD_OUT.iter().map(|q| (q, 1)));
    for (q, set) in asked {
        let t = &mut tally[set];
        let wanted: HashSet<String> = q.sources.iter().map(|k| ids[k].clone()).collect();
        // Retrieval on its own: is the source in the top five?
        let query_vec = block_on(mind.embed(&[q.text.to_string()]))
            .ok()
            .and_then(|v| v.into_iter().next());
        let top5 = h
            .engine()
            .retrieve(q.text, query_vec.as_deref(), 5)
            .unwrap_or_default();
        let rank = top5.iter().position(|hit| {
            hit.memory
                .receipts
                .iter()
                .any(|r| wanted.contains(&r.item_id))
        });
        if !q.unknown {
            t.recall_total += 1;
            t.recall_hits += usize::from(rank.is_some());
        }
        let a = asks.ask(&h, q.text);
        every_terminal &= a.phase != "timeout";
        let lower = plain(&a.text).to_lowercase();
        let ok = if q.unknown {
            matches!(a.phase.as_str(), "notSure" | "nothing")
        } else {
            a.phase == "done"
                && q.must
                    .iter()
                    .all(|group| group.iter().any(|w| lower.contains(w)))
        };
        t.correct += usize::from(ok);
        let cited_items: Vec<String> = a
            .receipts
            .iter()
            .filter_map(|r| r["itemId"].as_str().map(str::to_string))
            .collect();
        let supporting = cited_items.iter().filter(|i| wanted.contains(*i)).count();
        if !q.unknown {
            t.cited_all += cited_items.len();
            t.cited_ok += supporting;
        }
        let spans = bold_spans(&a.text);
        let cited_sentences: Vec<String> = a
            .memory_ids
            .iter()
            .filter_map(|id| {
                h.engine()
                    .get_memory(id)
                    .ok()
                    .map(|m| m.sentence.to_lowercase())
            })
            .collect();
        let mapped = spans
            .iter()
            .enumerate()
            .filter(|(i, s)| {
                let at = if a.memory_ids.len() == 1 { 0 } else { *i };
                cited_sentences
                    .get(at)
                    .is_some_and(|sentence| sentence.contains(&s.to_lowercase()))
            })
            .count();
        t.bold_total += spans.len();
        t.bold_mapped += mapped;
        if a.phase == "done" {
            t.bold_dropped += usize::from(a.plain);
            t.bold_answers += usize::from(
                !spans.is_empty() && spans.len() == a.memory_ids.len() && mapped == spans.len(),
            );
        }
        if let Some(ms_first) = a.first_sentence_ms {
            t.first_sentence.push(ms_first);
        }
        t.totals.push(a.total_ms);
        let cited_quotes: Vec<Value> = a
            .memory_ids
            .iter()
            .filter_map(|id| h.engine().get_memory(id).ok())
            .map(|m| {
                json!({
                    "memory": m.id, "sentence": m.sentence, "was": m.was,
                    "items": m.receipts.iter().map(|r| json!({
                        "item": r.item_id, "key": by_id.get(&r.item_id).copied().unwrap_or("?"),
                        "label": r.label, "span": r.span,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        let history = if q.id == "q02" || q.id == "q09" {
            let old = if q.id == "q02" {
                ["18 november", "wednesday"]
            } else {
                ["leeds", "leeds"]
            };
            let in_text = old.iter().any(|w| lower.contains(w));
            let in_memory = a.memory_ids.iter().any(|id| {
                h.engine()
                    .get_memory(id)
                    .map(|m| m.was.is_some())
                    .unwrap_or(false)
            });
            json!({"old_value_in_answer": in_text, "cited_memory_has_was": in_memory})
        } else {
            Value::Null
        };
        println!(
            "{} [{}] {} -> {} | {} | first sentence {} ms, total {} ms",
            q.id,
            q.kind,
            q.text,
            a.phase,
            if ok { "correct" } else { "WRONG" },
            a.first_sentence_ms.unwrap_or(0),
            a.total_ms
        );
        println!("    {}", plain(&a.text).replace('\n', " "));
        rows[set].push(answer_json(
            q,
            &a,
            json!({
                "correct": ok, "retrieval_rank_in_top5": rank.map(|r| r + 1),
                "top5": top5.iter().map(|hit| hit.memory.sentence.clone()).collect::<Vec<_>>(),
                "expected_items": q.sources, "cited_items": cited_items.len(),
                "cited_items_supporting": supporting, "bold_spans": spans,
                "bold_spans_supported_by_cited_memory": mapped, "cited": cited_quotes,
                "history": history,
            }),
        ));
        h.ok("get_status", json!({}));
    }
    let [rows, held_rows] = rows;
    run.check(
        "ask.terminal",
        "every question ends in exactly one terminal phase",
        every_terminal,
        "",
    );
    let q02_row = &rows[1]["judged"];
    run.check(
        "ask.changed_history",
        "the changed dentist appointment gives the new value and shows history",
        q02_row["correct"] == true
            && (q02_row["history"]["old_value_in_answer"] == true
                || q02_row["history"]["cited_memory_has_was"] == true),
        q02_row["history"].to_string(),
    );
    let unknown_ok = rows
        .iter()
        .find(|r| r["id"] == "q07")
        .is_some_and(|r| r["judged"]["correct"] == true);
    run.check(
        "ask.unknown",
        "a thing that was never kept gets 'not sure', not an invention",
        unknown_ok,
        "",
    );
    let receipts_everywhere = rows.iter().chain(&held_rows).all(|r| {
        !matches!(r["phase"].as_str(), Some("done"))
            || r["receipts"].as_array().is_some_and(|x| !x.is_empty())
    });
    run.check(
        "ask.receipts",
        "every answer that was given carries receipts",
        receipts_everywhere,
        "",
    );
    let metrics = tally[0].metrics(
        &rows,
        "12 questions on one seeded day; a tiny sample, not the 100-question benchmark",
    );
    let held_metrics = tally[1].metrics(
        &held_rows,
        "6 held-out questions, written before the extraction prompt changed and never tuned against",
    );
    run.put("questions", Value::Array(rows));
    run.put("metrics", metrics);
    run.put("held_out_questions", Value::Array(held_rows));
    run.put("held_out_metrics", held_metrics);

    // 5. Tidy and the morning page --------------------------------------------------------
    at(&h, PAGE_DAY, "03:00");
    host.set_ac(Some(true));
    host.set_idle(None);
    h.ok("tidy_anyway", json!({}));
    let tidy_started = Instant::now();
    let mut steps: Vec<String> = Vec::new();
    let mut tidy_report = Value::Null;
    let tidy_done = wait_until(env_secs("NOCTURNE_E2E_TIDY_SECS", 1800), || {
        let step = block_on(services.scheduler.tick());
        let label = format!("{step:?}");
        let label = label.chars().take(60).collect::<String>();
        if steps.last() != Some(&label) {
            steps.push(label);
        }
        if let SchedStep::Tidy(TidyStep::Finished(report)) = step {
            tidy_report = json!(format!("{report:?}"));
            return true;
        }
        false
    });
    let tidy_ms = ms(tidy_started.elapsed());
    run.check(
        "tidy.finished",
        "the overnight tidy runs to the end and writes the page",
        tidy_done.is_some(),
        format!("{} s; steps {steps:?}", tidy_ms / 1000),
    );
    run.put(
        "tidy",
        json!({"wall_ms": tidy_ms, "steps": steps, "report": tidy_report, "target": "< 10 minutes for one day"}),
    );

    let pages = h.ok("list_pages", json!({}));
    let date = pages
        .as_array()
        .and_then(|p| p.first())
        .and_then(|p| p["date"].as_str())
        .unwrap_or(PAGE_DAY)
        .to_string();
    let page = h.ok("get_page", json!({"date": date}));
    let lines: Vec<(&str, &Value)> = ["needsYou", "changed", "letGo", "upcoming", "learned"]
        .into_iter()
        .flat_map(|section| {
            page[section]
                .as_array()
                .into_iter()
                .flatten()
                .map(move |l| (section, l))
        })
        .collect();
    let without_receipts: Vec<String> = lines
        .iter()
        .filter(|(_, l)| {
            l["forgotten"] != true
                && (l["receipts"].as_array().is_none_or(Vec::is_empty)
                    || l["memoryIds"].as_array().is_none_or(Vec::is_empty))
        })
        .map(|(s, l)| format!("{s}: {}", l["text"]))
        .collect();
    let dangling: Vec<String> = lines
        .iter()
        .flat_map(|(_, l)| l["receipts"].as_array().into_iter().flatten())
        .filter_map(|r| r["itemId"].as_str())
        .filter(|id| h.call("get_item", json!({"id": id})).is_err())
        .map(str::to_string)
        .collect();
    run.check(
        "page.receipts",
        "every line on the page has a receipt that opens an original",
        !lines.is_empty() && without_receipts.is_empty() && dangling.is_empty(),
        format!(
            "{} lines; without receipts {without_receipts:?}; dangling {dangling:?}",
            lines.len()
        ),
    );
    let page_text = page.to_string().to_lowercase();
    let on_screen: Vec<&str> = PRIVATE_WORDS
        .iter()
        .copied()
        .filter(|w| page_text.contains(w))
        .collect();
    let audio = h.ok("get_edition", json!({"date": date, "edition": "audio"}));
    let print = h.ok("get_edition", json!({"date": date, "edition": "print"}));
    let leaks_in = |v: &Value| -> Vec<&str> {
        let t = v.to_string().to_lowercase();
        PRIVATE_WORDS
            .iter()
            .copied()
            .filter(|w| t.contains(w))
            .collect()
    };
    let public_needs = h.engine().needs_you_public().unwrap_or_default();
    let notification = notify::body(public_needs.len() as u32);
    let public_json = serde_json::to_value(&public_needs).unwrap_or_default();
    let status = h.ok("get_status", json!({}));
    let spoken = {
        let t = Instant::now();
        let r = h.call("speak_page", json!({"date": date}));
        (ms(t.elapsed()), r)
    };
    let spoken_leaks = match &spoken.1 {
        Ok(v) => leaks_in(v),
        Err(_) => Vec::new(),
    };
    run.check(
        "page.private",
        "the private note stays out of the audio and print editions, the notification and the status",
        leaks_in(&audio).is_empty()
            && leaks_in(&print).is_empty()
            && leaks_in(&public_json).is_empty()
            && PRIVATE_WORDS.iter().all(|w| !notification.to_lowercase().contains(w))
            && leaks_in(&status).is_empty()
            && spoken_leaks.is_empty(),
        format!(
            "on the screen edition (allowed by design: private is kept from audio, print, notifications and the tray): {on_screen:?}; audio {:?}; print {:?}; notification {notification:?}",
            leaks_in(&audio),
            leaks_in(&print)
        ),
    );
    run.check(
        "page.spoken",
        "the spoken page is made with the bundled voice and skips private lines",
        spoken
            .1
            .as_ref()
            .is_ok_and(|s| s["durationMs"].as_u64().unwrap_or(0) > 0),
        format!(
            "{} ms; {}",
            spoken.0,
            spoken
                .1
                .as_ref()
                .map(|s| format!(
                    "{} ms of audio, private skipped {}",
                    s["durationMs"], s["privateSkipped"]
                ))
                .unwrap_or_else(|e| e.to_string())
        ),
    );
    let which: Vec<&&Value> = lines
        .iter()
        .filter(|(s, l)| *s == "needsYou" && l["kind"] == "which")
        .map(|(_, l)| l)
        .collect();
    run.check(
        "page.which",
        "a which-is-right question appears for the contradiction",
        !which.is_empty(),
        which
            .iter()
            .map(|l| l["text"].to_string())
            .collect::<Vec<_>>()
            .join(" | "),
    );
    let changed_lines: Vec<String> = page["changed"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|l| l["text"].to_string())
        .collect();
    run.check(
        "page.changed",
        "the changed fact shows under 'changed'",
        !changed_lines.is_empty(),
        format!(
            "shown {}; cut by the page cap: {}; lines: {}",
            changed_lines.len(),
            page["moreChanged"],
            changed_lines.join(" | ")
        ),
    );
    run.check(
        "page.nothing_forgotten",
        "nothing forgotten appears (before forgetting anything)",
        lines.iter().all(|(_, l)| l["forgotten"] != true),
        "",
    );
    let page_words: HashSet<String> = lines
        .iter()
        .map(|(_, l)| l["text"].as_str().unwrap_or(""))
        .chain([
            page["headline"].as_str().unwrap_or(""),
            page["dek"].as_str().unwrap_or(""),
        ])
        .flat_map(words_of)
        .collect();
    let jargon: Vec<&str> = JARGON
        .iter()
        .copied()
        .filter(|j| page_words.contains(*j))
        .collect();
    run.check(
        "page.no_jargon",
        "the page uses no technical words",
        jargon.is_empty(),
        format!("{jargon:?}"),
    );
    run.check(
        "page.headline",
        "the page has a headline and a dek written by the real mind",
        page["headline"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty())
            && page["dek"].as_str().is_some_and(|s| !s.trim().is_empty()),
        format!("{} / {}", page["headline"], page["dek"]),
    );
    run.put(
        "page",
        json!({
            "date": date, "headline": page["headline"], "dek": page["dek"],
            "variant": page["variant"], "kept_count": page["keptCount"],
            "more_needs_you": page["moreNeedsYou"], "more_changed": page["moreChanged"],
            "changes_count": page["changesCount"],
            "lines": lines.iter().map(|(s, l)| json!({
                "section": s, "kind": l["kind"], "text": l["text"], "receipts": l["receipts"].as_array().map(Vec::len),
            })).collect::<Vec<_>>(),
            "question": page["question"], "private_on_screen_edition": on_screen,
            "private_skipped_in_audio": audio["privateSkipped"], "private_skipped_in_print": print["privateSkipped"],
            "notification_body": notification, "spoken_ms": spoken.0,
        }),
    );

    // 6. Forget one memory, then everything ------------------------------------------------
    let target = FORGET_TARGETS.iter().find_map(|t| {
        let found = h.ok(
            "list_memories",
            json!({"filter": {"itemId": ids[t.key], "limit": 50}}),
        );
        let memory_ids: Vec<String> = found
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m["id"].as_str().map(str::to_string))
            .collect();
        (!memory_ids.is_empty()).then_some((t, memory_ids))
    });
    if let Some((t, memory_ids)) = target {
        let before = asks.ask(&h, t.question);
        let before_text = plain(&before.text).to_lowercase();
        run.check(
            "forget.before",
            "before forgetting, the memory is answered with a receipt",
            before.phase == "done"
                && t.words.iter().any(|w| before_text.contains(w))
                && !before.receipts.is_empty(),
            format!("{}: {}", t.key, plain(&before.text)),
        );
        let preview = h.ok(
            "preview_forget",
            json!({"ids": memory_ids, "includeOriginals": true}),
        );
        let report = h.ok(
            "forget",
            json!({"previewToken": preview["token"].as_str().unwrap()}),
        );
        let after = asks.ask(&h, t.question);
        let after_text = plain(&after.text).to_lowercase();
        run.check(
            "forget.ask",
            "after forgetting, the memory is gone from answers",
            !t.words.iter().any(|w| after_text.contains(w))
                && after
                    .receipts
                    .iter()
                    .all(|r| r["itemId"] != ids[t.key].as_str()),
            format!("{} / {}", after.phase, plain(&after.text)),
        );
        let still: Vec<Value> = t
            .words
            .iter()
            .flat_map(|w| {
                h.ok(
                    "list_memories",
                    json!({"filter": {"query": w, "limit": 20}}),
                )
                .as_array()
                .cloned()
                .unwrap_or_default()
            })
            .collect();
        run.check(
            "forget.memories",
            "the forgotten memory is gone from the list of memories",
            still.is_empty() && report["memoriesRemoved"].as_u64().unwrap_or(0) >= 1,
            report.to_string(),
        );
        let editions = [
            h.ok("get_page", json!({"date": date})),
            h.ok("get_edition", json!({"date": date, "edition": "audio"})),
            h.ok("get_edition", json!({"date": date, "edition": "print"})),
        ];
        let gone_from_page = t.words.iter().all(|w| {
            editions
                .iter()
                .all(|e| !e.to_string().to_lowercase().contains(w))
        });
        run.check(
            "forget.page",
            "the forgotten memory is gone from the saved page in every edition",
            gone_from_page,
            "",
        );
        let item_gone = h.call("get_item", json!({"id": ids[t.key]})).is_err();
        run.put(
            "forget",
            json!({"target": t.key, "memories_forgotten": memory_ids.len(), "report": report,
                   "original_removed": item_gone, "question_before": before.text,
                   "question_after": after.text, "phase_after": after.phase}),
        );
    } else {
        run.check(
            "forget.before",
            "there is a memory to forget (none of the forget targets was read into a memory)",
            false,
            "",
        );
    }

    let models_bytes_end = dir_bytes(&engine_root.join("models"));
    let ledger_end = h.ok("get_privacy", json!({}))["ledger"]
        .as_array()
        .map(Vec::len)
        .unwrap_or(0);
    let downloads = block_on(async {
        let client = services.client().await?;
        client.downloads().await.ok()
    });
    run.check(
        "offline.after",
        "nothing was downloaded after setup (models folder and ledger unchanged, no download jobs)",
        models_bytes_end == models_bytes_after_setup
            && ledger_end == ledger_after_setup
            && downloads.as_ref().is_some_and(Vec::is_empty),
        format!(
            "models {models_bytes_after_setup} -> {models_bytes_end} bytes; ledger {ledger_after_setup} -> {ledger_end}"
        ),
    );
    let privacy = h.ok("get_privacy", json!({}));
    run.put(
        "outbound",
        json!({
            "app_counter": {"sent_to_internet_bytes": privacy["sentToInternetBytes"], "measured": privacy["sentMeasured"]},
            "models_bytes_after_setup": models_bytes_after_setup, "models_bytes_end": models_bytes_end,
            "ledger_after_setup": ledger_after_setup, "ledger_end": ledger_end,
            "note": "scripts/e2e-real.sh re-runs this test in a network namespace under strace for the byte count",
        }),
    );

    h.ok("delete_everything", json!({"confirmPhrase": DELETE_PHRASE}));
    let empty = [
        h.ok("list_items", json!({"filter": {}})),
        h.ok("list_memories", json!({"filter": {"limit": 50}})),
        h.ok("list_pages", json!({})),
    ]
    .iter()
    .all(|v| v.as_array().is_some_and(Vec::is_empty));
    let adv = h.ok("get_advanced", json!({}));
    let originals = dir_bytes(&data_dir.join("store/originals"));
    run.check(
        "delete.empty",
        "after deleting everything the store is empty",
        empty && adv["memoryCount"] == 0 && adv["itemCount"] == 0 && originals == 0,
        format!(
            "memories {} items {} original bytes {originals}",
            adv["memoryCount"], adv["itemCount"]
        ),
    );
    let after_delete = asks.ask(&h, "When is my dentist appointment?");
    run.check(
        "delete.ask",
        "after deleting everything, asking finds nothing",
        matches!(after_delete.phase.as_str(), "nothing" | "notSure"),
        after_delete.phase.clone(),
    );

    // 7. Quit ------------------------------------------------------------------------------
    let pgid = engine_pid.unwrap_or(0);
    let before_quit = group_members(pgid).len();
    let quit_started = Instant::now();
    h.state().run_exit_hooks();
    let quit_ms = ms(quit_started.elapsed());
    let gone = wait_until(Duration::from_secs(30), || {
        group_members(pgid).is_empty()
            && processes_mentioning(&engine_root.to_string_lossy()).is_empty()
    });
    let left = processes_mentioning(&engine_root.to_string_lossy());
    let gone_ms = gone.map(|d| ms(d) + quit_ms);
    run.check(
        "quit.clean",
        "no engine process is left running 3 s after quit",
        pgid != 0 && gone_ms.is_some_and(|t| t <= 3000) && left.is_empty(),
        format!(
            "{before_quit} processes in the engine's group before quit; quit call {quit_ms} ms; all gone after {} ms",
            gone_ms.unwrap_or(0)
        ),
    );
    run.put(
        "quit",
        json!({"engine_group_size_before": before_quit, "exit_hooks_ms": quit_ms,
               "gone_after_ms": gone_ms}),
    );

    // The table -----------------------------------------------------------------------------
    let summary = summary(&run);
    let _ = std::fs::write(run.dir.join(format!("summary{}.md", run.tag)), &summary);
    println!("\n{summary}");
    let failed: Vec<&Check> = run.checks.iter().filter(|c| !c.pass).collect();
    assert!(
        failed.is_empty(),
        "failed checks: {:?}",
        failed.iter().map(|c| c.id).collect::<Vec<_>>()
    );
}

/// What one set of questions measured.
#[derive(Default)]
struct Tally {
    recall_hits: usize,
    recall_total: usize,
    correct: usize,
    cited_ok: usize,
    cited_all: usize,
    first_sentence: Vec<u64>,
    totals: Vec<u64>,
    bold_total: usize,
    bold_mapped: usize,
    bold_answers: usize,
    bold_dropped: usize,
}

impl Tally {
    fn metrics(&self, rows: &[Value], sample: &str) -> Value {
        let answered = rows.iter().filter(|r| r["phase"] == "done").count();
        let (fs, tt) = (&self.first_sentence, &self.totals);
        json!({
            "sample": sample,
            "recall_at_5": {"hits": self.recall_hits, "of": self.recall_total, "value": ratio(self.recall_hits, self.recall_total), "target": "> 0.8"},
            "answer_accuracy": {"correct": self.correct, "of": rows.len(), "value": ratio(self.correct, rows.len()), "target": "> 0.75"},
            "citation_precision": {"supporting": self.cited_ok, "cited": self.cited_all, "value": ratio(self.cited_ok, self.cited_all), "target": "> 0.9"},
            "first_sentence_ms": {"p50": percentile(fs, 50.0), "p95": percentile(fs, 95.0), "max": fs.iter().max(), "target_p95": "< 3000"},
            "total_ms": {"p50": percentile(tt, 50.0), "p95": percentile(tt, 95.0), "max": tt.iter().max()},
            "bold_spans": {"in_answers": self.bold_total, "supported_by_cited_memory": self.bold_mapped, "answers_given": answered,
                           "answers_fully_covered": self.bold_answers, "answers_bold_dropped": self.bold_dropped},
        })
    }
}

fn ratio(a: usize, b: usize) -> Value {
    if b == 0 {
        Value::Null
    } else {
        json!((a as f64 / b as f64 * 1000.0).round() / 1000.0)
    }
}

/// Total memory in GiB, read from `/proc/meminfo` (zero where there is none).
fn total_memory_gib() -> u64 {
    let kib = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|t| {
            t.lines()
                .find_map(|l| l.strip_prefix("MemTotal:"))
                .and_then(|r| {
                    r.split_whitespace()
                        .next()
                        .and_then(|n| n.parse::<u64>().ok())
                })
        })
        .unwrap_or(0);
    kib >> 20
}

fn summary(run: &Run) -> String {
    let o = &run.out;
    let mut s = String::new();
    s.push_str("# Real end-to-end run\n\n");
    s.push_str(&format!(
        "One seeded day, the small tier, real models. {} checks, {} failed. Wall time {} s.\n\n",
        run.checks.len(),
        run.checks.iter().filter(|c| !c.pass).count(),
        run.started.elapsed().as_secs()
    ));
    s.push_str("| Check | Result | Detail |\n|---|---|---|\n");
    for c in &run.checks {
        let detail: String = c.detail.chars().take(160).collect();
        s.push_str(&format!(
            "| {} | {} | {} |\n",
            c.what,
            if c.pass { "pass" } else { "FAIL" },
            detail.replace('|', "/")
        ));
    }
    let m = &o["metrics"];
    s.push_str("\n## Against docs/design/evaluation.md (12 questions, a tiny sample)\n\n");
    s.push_str("| Metric | Measured | Target |\n|---|---|---|\n");
    for (name, key) in [
        ("Recall@5", "recall_at_5"),
        ("Answer accuracy", "answer_accuracy"),
        ("Citation precision", "citation_precision"),
    ] {
        s.push_str(&format!(
            "| {name} | {} ({} of {}) | {} |\n",
            m[key]["value"],
            m[key]["hits"]
                .as_u64()
                .or(m[key]["correct"].as_u64())
                .or(m[key]["supporting"].as_u64())
                .unwrap_or(0),
            m[key]["of"]
                .as_u64()
                .or(m[key]["cited"].as_u64())
                .unwrap_or(0),
            m[key]["target"]
        ));
    }
    s.push_str(&format!(
        "| p95 time to first sentence | {} ms (p50 {} ms, max {} ms) | < 3000 ms |\n",
        m["first_sentence_ms"]["p95"], m["first_sentence_ms"]["p50"], m["first_sentence_ms"]["max"]
    ));
    s.push_str(&format!(
        "| p95 total answer time | {} ms (p50 {} ms) | n/a |\n",
        m["total_ms"]["p95"], m["total_ms"]["p50"]
    ));
    s.push_str(&format!(
        "| Overnight tidy wall time | {} s | < 600 s |\n",
        o["tidy"]["wall_ms"].as_u64().unwrap_or(0) / 1000
    ));
    s.push_str(&format!(
        "| Reading, {} items | {} s | n/a |\n",
        o["reading"]["items"].as_array().map(Vec::len).unwrap_or(0),
        o["reading"]["wall_ms"].as_u64().unwrap_or(0) / 1000
    ));
    s.push_str(&format!(
        "| Bold spans in answers | {} ({} supported by the cited memory) | n/a |\n",
        m["bold_spans"]["in_answers"], m["bold_spans"]["supported_by_cited_memory"]
    ));
    s.push_str(&format!(
        "| Answers with one bold span per receipt | {} of {} given ({} shown without bold) | n/a |\n",
        m["bold_spans"]["answers_fully_covered"],
        m["bold_spans"]["answers_given"],
        m["bold_spans"]["answers_bold_dropped"]
    ));
    let h = &o["held_out_metrics"];
    s.push_str(
        "\n## Held out (6 questions, never tuned against)\n\n| Metric | Measured |\n|---|---|\n",
    );
    s.push_str(&format!(
        "| Recall@5 | {} ({} of {}) |\n| Answer accuracy | {} ({} of {}) |\n| Citation precision | {} ({} of {}) |\n",
        h["recall_at_5"]["value"], h["recall_at_5"]["hits"], h["recall_at_5"]["of"],
        h["answer_accuracy"]["value"], h["answer_accuracy"]["correct"], h["answer_accuracy"]["of"],
        h["citation_precision"]["value"], h["citation_precision"]["supporting"], h["citation_precision"]["cited"],
    ));
    s.push_str("\n## Questions\n\n| # | Kind | Phase | Correct | Recall rank | First sentence | Total |\n|---|---|---|---|---|---|---|\n");
    let all_questions = o["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(o["held_out_questions"].as_array().into_iter().flatten());
    for q in all_questions {
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} ms | {} ms |\n",
            q["id"].as_str().unwrap_or(""),
            q["kind"].as_str().unwrap_or(""),
            q["phase"].as_str().unwrap_or(""),
            q["judged"]["correct"],
            q["judged"]["retrieval_rank_in_top5"],
            q["first_sentence_ms"],
            q["total_ms"]
        ));
    }
    s.push_str("\n## Items read\n\n| Item | Kind | Status | Memories | Reading time |\n|---|---|---|---|---|\n");
    for i in o["reading"]["items"].as_array().into_iter().flatten() {
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} ms |\n",
            i["key"].as_str().unwrap_or(""),
            i["kind"].as_str().unwrap_or(""),
            i["status"].as_str().unwrap_or(""),
            i["memories"],
            i["since_previous_ms"]
        ));
    }
    s
}
