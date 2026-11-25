//! The stand-in provider: loopback only, scripted answers and errors, and a record of every
//! request.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

use serde_json::{Value, json};

struct StandIn {
    child: Child,
    base: String,
}

impl Drop for StandIn {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start() -> StandIn {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fake-provider"))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let port: u16 = line.trim().strip_prefix("ready ").unwrap().parse().unwrap();
    StandIn {
        child,
        base: format!("http://127.0.0.1:{port}"),
    }
}

#[test]
fn refuses_any_address_but_loopback() {
    for host in ["0.0.0.0", "::", "192.168.1.2", "localhost"] {
        let status = Command::new(env!("CARGO_BIN_EXE_fake-provider"))
            .args(["--host", host])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(2), "{host}");
    }
}

#[tokio::test]
async fn answers_from_the_script_and_records_every_request() {
    let s = start();
    let http = nocturne_lemonade::tls::client();
    let models: Value = http
        .get(format!("{}/v1/models", s.base))
        .bearer_auth("sk-test-key")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(models["data"][0]["id"], "stand-in-model");

    let body = json!({"model": "stand-in-model", "messages": [{"role": "user", "content": "q"}]});
    let reply: Value = http
        .post(format!("{}/v1/chat/completions", s.base))
        .json(&body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        reply["choices"][0]["message"]["content"]
            .as_str()
            .unwrap()
            .contains("[m1]")
    );
    assert!(reply["usage"].get("cost").is_none());

    http.post(format!("{}/fake/script", s.base))
        .json(&json!({"cost": 0.0021, "reply": "Tuesday [m2]."}))
        .send()
        .await
        .unwrap();
    let streamed = http
        .post(format!("{}/v1/chat/completions", s.base))
        .json(&json!({"model": "m", "stream": true, "messages": []}))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(streamed.contains("Tuesday [m2].") && streamed.contains("\"cost\":0.0021"));
    assert!(streamed.ends_with("data: [DONE]\n\n"));

    let log: Value = http
        .get(format!("{}/fake/requests", s.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let paths: Vec<&str> = log
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        ["/v1/models", "/v1/chat/completions", "/v1/chat/completions"]
    );
    assert_eq!(log[0]["headers"]["authorization"], "Bearer sk-test-key");
    assert_eq!(log[1]["body"], body);
}

#[tokio::test]
async fn scripted_errors_echo_the_key_and_the_marker() {
    let s = start();
    let http = nocturne_lemonade::tls::client();
    for status in [401u16, 402, 403, 404, 429, 500] {
        for multiline in [false, true] {
            http.post(format!("{}/fake/script", s.base))
                .json(&json!({"status": status, "marker": "MARK-7", "multiline": multiline}))
                .send()
                .await
                .unwrap();
            let r = http
                .post(format!("{}/v1/chat/completions", s.base))
                .bearer_auth("sk-echo-me")
                .json(&json!({"model": "m", "messages": []}))
                .send()
                .await
                .unwrap();
            assert_eq!(r.status().as_u16(), status);
            let text = r.text().await.unwrap();
            assert!(
                text.contains("MARK-7") && text.contains("sk-echo-me"),
                "{text}"
            );
            assert_eq!(text.contains("\\n"), multiline, "{text}");
        }
    }
    http.post(format!("{}/fake/script", s.base))
        .json(&json!({"status": null, "empty": true}))
        .send()
        .await
        .unwrap();
    let reply: Value = http
        .post(format!("{}/v1/chat/completions", s.base))
        .json(&json!({"model": "m", "messages": []}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reply["choices"][0]["message"]["content"], "");
    let refused = http
        .post(format!("{}/fake/relay", s.base))
        .json(&json!({"url": "https://api.example.com/v1", "key": "k"}))
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status().as_u16(), 400, "relays to loopback only");
}

#[tokio::test]
async fn a_stray_probe_from_another_program_is_not_recorded() {
    let s = start();
    let http = nocturne_lemonade::tls::client();
    for agent in ["Go-http-client/1.1", "nocturne-test"] {
        let _ = http
            .get(format!("{}/", s.base))
            .header("user-agent", agent)
            .send()
            .await
            .unwrap();
    }
    let log: Value = http
        .get(format!("{}/fake/requests", s.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let agents: Vec<&str> = log
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["headers"]["user-agent"].as_str().unwrap())
        .collect();
    assert_eq!(agents, ["nocturne-test"]);
}
