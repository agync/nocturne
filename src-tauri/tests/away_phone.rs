//! The Rust test phone, played against another computer: `ui/interop` starts one written in
//! TypeScript (the phone app's own test computer) and runs this once per scenario. It pairs, opens
//! a session, keeps a note and a photo, asks, lists pages, and prints what happened as one line
//! starting with `AWAYPHONE `.
//!
//! Not part of a normal test run: it needs the scenario in `AWAY_SCENARIO` (JSON).

use nocturne_app_lib::remote::protocol::CHUNK;
use nocturne_app_lib::remote::testing::PhonePair;
use nocturne_app_lib::remote::testing::client::{Addr, Mode, Opened, Pairing, PhoneClient};
use nocturne_contract::{AskPhase, RemoteAnswer, RemoteCall};
use serde_json::{Value, json};

fn photo(marker: &str, len: usize) -> Vec<u8> {
    let mut b = vec![0xff, 0xd8, 0xff, 0xe0, 0, 0x10, b'J', b'F', b'I', b'F'];
    b.extend_from_slice(marker.as_bytes());
    b.extend((0..len).map(|i| (i * 31 % 251) as u8));
    b
}

async fn run(s: Value) -> Result<Value, String> {
    let port = s["port"].as_u64().ok_or("port")? as u16;
    let addr = Addr::Tcp(port);
    let mode = if s["mode"] == "poll" {
        Mode::LongPoll
    } else {
        Mode::WebSocket
    };
    let name = s["name"].as_str().unwrap_or("Rust phone");
    let phone = match s["link"].as_str() {
        Some(link) => PhonePair::scanned(link, name),
        None => PhonePair::typed(
            s["code"].as_str().ok_or("code")?,
            s["host"].as_str().ok_or("host")?,
            name,
        ),
    };
    let pairing = Pairing::start(&addr, phone, mode)
        .await
        .map_err(|e| format!("pairing start: {e:?}"))?;
    let words = pairing.phone.words();
    let keys = pairing
        .outcome()
        .await
        .map_err(|e| format!("pairing outcome: {e:?}"))?
        .ok_or("refused")?;
    let Opened::Session(mut c, hello) = PhoneClient::open(&addr, &keys, mode).await else {
        return Err("no session".into());
    };
    let status = c
        .call(RemoteCall::GetStatus)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let text = s["text"].as_str().unwrap_or("note");
    let kept = c
        .call(RemoteCall::KeepText {
            text: text.into(),
            client_id: format!("{name}-text"),
            feeling: None,
            good: None,
            captured_at: None,
        })
        .await
        .map_err(|e| format!("{e:?}"))?;
    let bytes = photo(s["marker"].as_str().unwrap_or("MARK"), 2 * CHUNK + 100);
    let id = c
        .request(RemoteCall::KeepBytes {
            mime: "image/jpeg".into(),
            size: bytes.len() as f64,
            client_id: format!("{name}-photo"),
            caption: Some("a photo".into()),
            feeling: None,
            good: None,
            captured_at: None,
        })
        .await
        .map_err(|e| format!("{e:?}"))?;
    let parts: Vec<&[u8]> = bytes.chunks(CHUNK).collect();
    for (i, p) in parts.iter().enumerate() {
        c.chunk(id, i as u32, i + 1 == parts.len(), p)
            .await
            .map_err(|e| format!("{e:?}"))?;
    }
    let photo_kept = c.answer(id).await.map_err(|e| format!("{e:?}"))?;
    let ask_id = c
        .request(RemoteCall::Ask {
            question: "When is swimming?".into(),
        })
        .await
        .map_err(|e| format!("{e:?}"))?;
    let mut streamed = String::new();
    let mut phases = Vec::new();
    loop {
        match c.answer(ask_id).await.map_err(|e| format!("ask: {e:?}"))? {
            RemoteAnswer::AskStarted { .. } => {}
            RemoteAnswer::AskDelta { delta } => {
                streamed.push_str(delta.text_delta.as_deref().unwrap_or(""));
                phases.push(format!("{:?}", delta.phase));
                if matches!(delta.phase, AskPhase::Done) {
                    break;
                }
            }
            other => return Err(format!("ask: {other:?}")),
        }
    }
    let pages = c
        .call(RemoteCall::ListPages)
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(json!({
        "words": words,
        "hello": serde_json::to_value(&hello).unwrap(),
        "status": serde_json::to_value(&status).unwrap(),
        "kept": serde_json::to_value(&kept).unwrap(),
        "photoKept": serde_json::to_value(&photo_kept).unwrap(),
        "photoSize": bytes.len(),
        "streamed": streamed,
        "phases": phases,
        "pages": serde_json::to_value(&pages).unwrap(),
    }))
}

#[test]
#[ignore = "driven by ui/interop with AWAY_SCENARIO"]
fn phone() {
    let scenario: Value =
        serde_json::from_str(&std::env::var("AWAY_SCENARIO").expect("AWAY_SCENARIO")).unwrap();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let out = match rt.block_on(run(scenario)) {
        Ok(v) => json!({"ok": v}),
        Err(e) => json!({"error": e}),
    };
    println!("AWAYPHONE {out}");
}
