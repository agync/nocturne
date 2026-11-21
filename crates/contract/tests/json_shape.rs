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
            feeling: None,
            good: None,
            captured_at: None,
        },
    };
    assert_eq!(
        serde_json::to_string(&req).unwrap(),
        r#"{"id":3,"call":{"cmd":"keepText","text":"milk","clientId":"c-1"}}"#
    );
    let marked = RemoteCall::KeepText {
        text: "milk".into(),
        client_id: "c-1".into(),
        feeling: Some(Feeling {
            part: FeelingPart::Settled,
            point: FeelingPoint {
                pleasant: 0.5,
                energy: -0.5,
            },
            words: vec!["calm".into()],
        }),
        good: Some(true),
        captured_at: None,
    };
    assert_eq!(
        serde_json::to_string(&marked).unwrap(),
        r#"{"cmd":"keepText","text":"milk","clientId":"c-1","feeling":{"part":"settled","point":{"pleasant":0.5,"energy":-0.5},"words":["calm"]},"good":true}"#
    );
    let older: RemoteCall = serde_json::from_str(
        r#"{"cmd":"keepRecording","mime":"audio/wav","size":4,"clientId":"c"}"#,
    )
    .unwrap();
    assert!(matches!(
        older,
        RemoteCall::KeepRecording {
            feeling: None,
            good: None,
            captured_at: None,
            ..
        }
    ));
    let waited: RemoteCall = serde_json::from_str(
        r#"{"cmd":"keepText","text":"x","clientId":"c","capturedAt":"2026-11-12T22:50:00Z"}"#,
    )
    .unwrap();
    assert!(matches!(
        &waited,
        RemoteCall::KeepText { captured_at: Some(at), .. } if at == "2026-11-12T22:50:00Z"
    ));
    assert!(
        serde_json::to_string(&waited)
            .unwrap()
            .ends_with(r#""capturedAt":"2026-11-12T22:50:00Z"}"#)
    );
    let day: RemoteCall =
        serde_json::from_str(r#"{"cmd":"getJournalDay","date":"2026-11-19"}"#).unwrap();
    assert_eq!(
        day,
        RemoteCall::GetJournalDay {
            date: Some("2026-11-19".into())
        }
    );
    let same: RemoteCall = serde_json::from_str(
        r#"{"cmd":"answerSameName","questionId":"snq_1","answer":{"type":"entity","entityId":"ent_1"}}"#,
    )
    .unwrap();
    assert!(matches!(same, RemoteCall::AnswerSameName { .. }));
    let journal: RemoteCall = serde_json::from_str(
        r#"{"cmd":"answerJournalQuestion","pageDate":"2026-11-20","choice":"notGreat","clientId":"c1"}"#,
    )
    .unwrap();
    assert_eq!(
        journal,
        RemoteCall::AnswerJournalQuestion {
            page_date: "2026-11-20".into(),
            choice: nocturne_contract::QuestionChoice::NotGreat,
            line: None,
            client_id: "c1".into(),
        }
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
    assert_eq!(OnlineLimit::default().kind, LimitKind::None);
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

#[test]
fn phone_sends_are_tagged_and_read_by_older_shapes() {
    let call = RemoteCall::SendReadings {
        batch: ReadingBatch {
            sync_id: "s1".into(),
            seq: 0,
            last: true,
            changes: vec![
                ReadingChange::Upsert {
                    reading: Reading {
                        phone_id: "r1".into(),
                        day: "2026-11-20".into(),
                        start: "2026-11-20T07:00:00+00:00".into(),
                        end: None,
                        source_device: None,
                        value: ReadingValue::Steps { total: 8412 },
                    },
                },
                ReadingChange::Removed {
                    phone_id: "r0".into(),
                    kind: ReadingKind::RestingHeartRate,
                },
            ],
        },
    };
    assert_eq!(
        serde_json::to_string(&call).unwrap(),
        r#"{"cmd":"sendReadings","batch":{"syncId":"s1","seq":0,"last":true,"changes":[{"type":"upsert","reading":{"phoneId":"r1","day":"2026-11-20","start":"2026-11-20T07:00:00+00:00","end":null,"sourceDevice":null,"value":{"type":"steps","total":8412}}},{"type":"removed","phoneId":"r0","kind":"restingHeartRate"}]}}"#
    );
    let synced = RemoteAnswer::Synced {
        receipt: SyncReceipt {
            taken: 1,
            unchanged: 0,
            refused: 0,
            stopped: false,
            too_many: false,
        },
    };
    assert_eq!(
        serde_json::to_string(&synced).unwrap(),
        r#"{"type":"synced","receipt":{"taken":1,"unchanged":0,"refused":0,"stopped":false,"tooMany":false}}"#
    );

    // A computer from before phone apps says hello without features.
    let hello: PhoneHello = serde_json::from_str(
        r#"{"computerName":"c","deviceName":"d","connection":"simple","version":"0.7.2","minPhoneBuild":1}"#,
    )
    .unwrap();
    assert!(hello.features.is_empty());
}

#[test]
fn the_page_journal_and_check_in_read_without_the_phone_fields() {
    let check_in: CheckIn = serde_json::from_str(
        r#"{"id":"i","keptAt":"2026-11-20T08:00:00+00:00","day":"2026-11-20",
            "feeling":{"part":"settled","point":{"pleasant":0,"energy":0},"words":[]},
            "line":null,"urge":null,"about":null,"tried":[],"fromDevice":null,"mentions":[],
            "leftOut":false}"#,
    )
    .unwrap();
    assert_eq!(check_in.after_sleep_min, None);
    assert!(
        !serde_json::to_string(&check_in)
            .unwrap()
            .contains("afterSleepMin")
    );
    let day: JournalDay = serde_json::from_str(
        r#"{"day":"2026-11-20","today":true,"firstUse":false,"entry":[],"entrySavedAt":null,
            "entryReadAt":null,"rows":[],"checkins":[],"good":[],"people":[],"reflection":null,
            "reading":0,"sameName":[]}"#,
    )
    .unwrap();
    assert_eq!(day.health, None);
}
