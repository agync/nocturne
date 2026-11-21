//! Typed access to the reference vectors for the phone channel
//! (`src-tauri/tests/fixtures/remote-vectors.json`, written by `scripts/remote_vectors.py`).
//!
//! Read by the crate's unit tests through `protocol::reference`.
//! Bytes in the fixture are lowercase hex. A positive vector has `inputs`, `intermediate` and
//! `output`; a negative one has `inputs` and the refusal it must get in `expect`.
#![allow(dead_code)]

use serde_json::{Map, Value};

const FIXTURE: &str = include_str!("../../../../src-tauri/tests/fixtures/remote-vectors.json");

pub mod hex {
    pub fn decode(s: &str) -> Vec<u8> {
        assert!(s.len().is_multiple_of(2), "odd hex length: {s}");
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
            .collect()
    }

    pub fn encode(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }
}

pub struct Vectors {
    doc: Value,
}

pub fn load() -> Vectors {
    Vectors {
        doc: serde_json::from_str(FIXTURE).expect("the reference vectors parse"),
    }
}

impl Vectors {
    pub fn constants(&self) -> &Map<String, Value> {
        self.doc["constants"].as_object().expect("constants")
    }

    /// Every refusal code a negative vector may expect.
    pub fn refusals(&self) -> Vec<&str> {
        self.doc["refusals"]
            .as_array()
            .expect("refusals")
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect()
    }

    pub fn words_sha256(&self) -> &str {
        self.doc["words"]["sha256"].as_str().expect("words sha256")
    }

    pub fn key_scalar(&self, name: &str) -> Vec<u8> {
        hex::decode(self.doc["keys"][name]["scalar"].as_str().expect("key"))
    }

    pub fn key_public(&self, name: &str) -> Vec<u8> {
        hex::decode(self.doc["keys"][name]["public"].as_str().expect("key"))
    }

    pub fn positives(&self) -> Vec<Vector<'_>> {
        list(&self.doc["positive"])
    }

    pub fn negatives(&self) -> Vec<Vector<'_>> {
        list(&self.doc["negative"])
    }

    pub fn positive(&self, name: &str) -> Vector<'_> {
        find(self.positives(), name)
    }

    pub fn negative(&self, name: &str) -> Vector<'_> {
        find(self.negatives(), name)
    }

    /// The vectors of one step (`"data"`, `"words"`, `"publicKey"`, ...).
    pub fn step(&self, step: &str, positive: bool) -> Vec<Vector<'_>> {
        let all = if positive {
            self.positives()
        } else {
            self.negatives()
        };
        all.into_iter().filter(|v| v.step == step).collect()
    }
}

fn list(v: &Value) -> Vec<Vector<'_>> {
    v.as_array()
        .expect("vector list")
        .iter()
        .map(|v| Vector {
            name: v["name"].as_str().expect("name"),
            step: v["step"].as_str().expect("step"),
            raw: v,
        })
        .collect()
}

fn find<'a>(all: Vec<Vector<'a>>, name: &str) -> Vector<'a> {
    all.into_iter()
        .find(|v| v.name == name)
        .unwrap_or_else(|| panic!("no reference vector named {name}"))
}

#[derive(Clone, Copy)]
pub struct Vector<'a> {
    pub name: &'a str,
    pub step: &'a str,
    pub raw: &'a Value,
}

impl<'a> Vector<'a> {
    pub fn input(&self, key: &str) -> &'a Value {
        field(self.raw, "inputs", key, self.name)
    }

    pub fn intermediate(&self, key: &str) -> &'a Value {
        field(self.raw, "intermediate", key, self.name)
    }

    pub fn output(&self, key: &str) -> &'a Value {
        field(self.raw, "output", key, self.name)
    }

    pub fn input_hex(&self, key: &str) -> Vec<u8> {
        hex::decode(self.input(key).as_str().expect("hex input"))
    }

    pub fn intermediate_hex(&self, key: &str) -> Vec<u8> {
        hex::decode(self.intermediate(key).as_str().expect("hex intermediate"))
    }

    pub fn output_hex(&self, key: &str) -> Vec<u8> {
        hex::decode(self.output(key).as_str().expect("hex output"))
    }

    pub fn input_str(&self, key: &str) -> &'a str {
        self.input(key).as_str().expect("string input")
    }

    /// The refusal a negative vector must get.
    pub fn expect(&self) -> &'a str {
        self.raw["expect"]
            .as_str()
            .expect("negative vectors name a refusal")
    }
}

fn field<'a>(raw: &'a Value, part: &str, key: &str, name: &str) -> &'a Value {
    raw[part]
        .get(key)
        .unwrap_or_else(|| panic!("vector {name} has no {part}.{key}"))
}
