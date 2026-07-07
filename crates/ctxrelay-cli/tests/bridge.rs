use ctxrelay_cli::bridge::CaptureRequest;

#[test]
fn deserializes_a_capture_request_matching_the_schema() {
    let raw = r#"
    {
      "version": "1",
      "token": "abc123",
      "conversation_id": "fca79960-3026-40e1-beba-6abb33fe20d5",
      "org_id": "ed9a9a3c-9d81-43a0-b974-3aa686e20a87",
      "captured_at": "2026-07-07T01:00:00Z",
      "snapshot": { "uuid": "fca79960-3026-40e1-beba-6abb33fe20d5", "chat_messages": [] }
    }
    "#;

    let request: CaptureRequest = serde_json::from_str(raw).expect("should deserialize per bridge-protocol schema");

    assert_eq!(request.version, "1");
    assert_eq!(request.token, "abc123");
    assert_eq!(request.conversation_id, "fca79960-3026-40e1-beba-6abb33fe20d5");
    assert_eq!(request.org_id, "ed9a9a3c-9d81-43a0-b974-3aa686e20a87");
}

#[test]
fn rejects_a_request_missing_a_required_field() {
    let raw = r#"{ "version": "1", "token": "abc123" }"#;

    let result: Result<CaptureRequest, _> = serde_json::from_str(raw);

    assert!(result.is_err(), "conversation_id/org_id/snapshot are required by the schema");
}
