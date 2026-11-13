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
