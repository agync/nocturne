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
