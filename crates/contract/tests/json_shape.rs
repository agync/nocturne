use nocturne_contract::*;

#[test]
fn tagged_enums_and_camel_case() {
    let span = Span::Time {
        start_ms: 0,
        end_ms: 10_000,
    };
    assert_eq!(
        serde_json::to_string(&span).unwrap(),
        r#"{"type":"time","startMs":0,"endMs":10000}"#
    );
    assert_eq!(
        serde_json::to_string(&ItemStatus::CouldntRead).unwrap(),
        r#""couldntRead""#
    );
}

#[test]
fn event_names_are_unique() {
    let mut names = ALL_EVENTS.to_vec();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), ALL_EVENTS.len());
}

#[test]
fn remote_calls_are_tagged_by_cmd() {
    let req = RemoteRequest {
        id: 3,
        call: RemoteCall::KeepText {
            text: "milk".into(),
            client_id: "c-1".into(),
        },
    };
    assert_eq!(
        serde_json::to_string(&req).unwrap(),
        r#"{"id":3,"call":{"cmd":"keepText","text":"milk","clientId":"c-1"}}"#
    );
    let unit: RemoteCall = serde_json::from_str(r#"{"cmd":"listPages"}"#).unwrap();
    assert_eq!(unit, RemoteCall::ListPages);
    assert!(serde_json::from_str::<RemoteCall>(r#"{"cmd":"deleteEverything"}"#).is_err());
}

#[test]
fn remote_state_and_answers_are_tagged_by_type() {
    let on = RemoteState::On {
        address: "a.example".into(),
        machine_name: None,
    };
    assert_eq!(
        serde_json::to_string(&on).unwrap(),
        r#"{"type":"on","address":"a.example","machineName":null}"#
    );
    let reply = RemoteReply {
        id: 0,
        answer: RemoteAnswer::Error {
            code: PhoneError::SessionExpired,
        },
    };
    assert_eq!(
        serde_json::to_string(&reply).unwrap(),
        r#"{"id":0,"answer":{"type":"error","code":"sessionExpired"}}"#
    );
    assert_eq!(
        serde_json::to_string(&AutoOff::Unused30Days).unwrap(),
        r#""unused30Days""#
    );
}

#[test]
fn online_types_are_tagged_and_keyless() {
    let by = AnsweredBy::Computer {
        reason: Some(LocalReason::KeptHere),
    };
    assert_eq!(
        serde_json::to_string(&by).unwrap(),
        r#"{"type":"computer","reason":"keptHere"}"#
    );
    let paused = OnlineState::Paused {
        pause: OnlinePause::Limit {
            until: "2026-12-01T00:00:00+00:00".into(),
        },
    };
    assert_eq!(
        serde_json::to_string(&paused).unwrap(),
        r#"{"type":"paused","pause":{"type":"limit","until":"2026-12-01T00:00:00+00:00"}}"#
    );
    assert_eq!(
        serde_json::to_string(&SentLine::Memory {
            n: 2,
            text: None,
            state: SentMemory::Changed,
        })
        .unwrap(),
        r#"{"type":"memory","n":2,"text":null,"state":"changed"}"#
    );
    assert_eq!(AskPlace::default(), AskPlace::Auto);
    assert_eq!(OnlineLimit::default().questions, 200);
    let form: OnlineForm = serde_json::from_str(
        r#"{"baseUrl":"https://api.example.com/v1","key":"sk-secret-value","model":"m"}"#,
    )
    .unwrap();
    assert_eq!(form.base_url, "https://api.example.com/v1");
    assert!(!format!("{form:?}").contains("sk-secret-value"));
    assert_eq!(
        serde_json::to_string(&CheckOutcome::AddressNotAllowed).unwrap(),
        r#"{"type":"addressNotAllowed"}"#
    );
}

#[test]
fn ask_delta_reads_without_the_online_fields() {
    let delta: AskDelta = serde_json::from_str(
        r#"{"askId":"a","phase":"done","textDelta":null,"receipts":[],"memoryIds":[],
            "near":[],"found":null,"etaSeconds":null}"#,
    )
    .unwrap();
    assert_eq!(delta.answered_by, None);
    assert_eq!(delta.left_out, None);
    assert_eq!(delta.send_id, None);
}

#[test]
fn model_types_are_tagged() {
    let reason = FitReason::Disk { short_bytes: 1.0 };
    assert_eq!(
        serde_json::to_string(&reason).unwrap(),
        r#"{"type":"disk","shortBytes":1.0}"#
    );
    let phase = ChangePhase::Downloading {
        received: 1.0,
        total: 2.0,
        paused: false,
    };
    assert_eq!(
        serde_json::to_string(&phase).unwrap(),
        r#"{"type":"downloading","received":1.0,"total":2.0,"paused":false}"#
    );
}
