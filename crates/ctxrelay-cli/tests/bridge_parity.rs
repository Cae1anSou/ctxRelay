use serde_json::Value;

/// `bridge-protocol/schema.json` 是 CLI ↔ 扩展契约的唯一权威来源(见架构文档
/// §10.1),但 Rust 端(`bridge.rs`)和 TS 端(`extension/src/background.ts`)目前
/// 都是手写的投影,没有代码生成兜底——这意味着任何一边改了字段名而没改另一边,
/// `cargo test` 和 `npm run build` 各自都不会报错,只会在真机跑起来时才炸(架构
/// 文档 §1 明确点名要避免的那种"静默失败")。
///
/// 这条测试不是完整的类型系统一致性证明(没有真的解析 TS AST),但足够便宜、足够
/// 有效地抓住最常见的漂移方式:直接把 schema.json 里某个 definition 的 `required`
/// 列出的每个字段名,当作字符串去两边源码里找——改名字的话,新名字不会同时出现
/// 在三处,这条测试就会红。`CaptureRequest`/`CaptureResponse` 都要覆盖。
///
/// 对 schema 里某个 definition 的 `required` 字段列表,断言每个字段名都同时出现在
/// Rust 和 TS 两侧的源码字符串里。
fn assert_required_fields_appear_in_both(
    schema: &Value,
    definition_name: &str,
    rust_source: &str,
    rust_file: &str,
    ts_source: &str,
    ts_file: &str,
) {
    let required = schema["definitions"][definition_name]["required"]
        .as_array()
        .unwrap_or_else(|| panic!("{definition_name}.required must be an array"));

    for field in required {
        let field_name = field
            .as_str()
            .expect("required field entries must be strings");
        assert!(
            rust_source.contains(field_name),
            "schema.json requires {definition_name}.{field_name:?} but it's missing from \
             {rust_file} — {definition_name}'s Rust projection has drifted from the schema"
        );
        assert!(
            ts_source.contains(field_name),
            "schema.json requires {definition_name}.{field_name:?} but it's missing from \
             {ts_file} — {definition_name}'s TypeScript projection has drifted from the schema"
        );
    }
}

#[test]
fn capture_request_required_fields_appear_in_both_rust_and_typescript() {
    let schema_raw = std::fs::read_to_string("../../bridge-protocol/schema.json")
        .expect("bridge-protocol/schema.json must exist");
    let schema: Value = serde_json::from_str(&schema_raw).expect("schema.json must be valid JSON");

    let rust_source = std::fs::read_to_string("src/bridge.rs").expect("src/bridge.rs must exist");
    let ts_source = std::fs::read_to_string("../../extension/src/bridge.ts")
        .expect("extension/src/bridge.ts must exist");

    assert_required_fields_appear_in_both(
        &schema,
        "CaptureRequest",
        &rust_source,
        "crates/ctxrelay-cli/src/bridge.rs",
        &ts_source,
        "extension/src/bridge.ts",
    );
}

/// 覆盖响应体那一侧——在此之前 `CaptureResponse` 只在 `bridge-protocol/schema.json`
/// 里定义过,Rust 侧连手写 struct 都没有(响应是在 `main.rs` 里用 `format!` 拼字符串
/// 出来的),TS 侧也完全不解析响应体。两边现在都补了 `CaptureResponse` 类型,这条
/// 测试确保它别重蹈 `CaptureRequest` 曾经的覆辙——字段改名了却没有任何测试能发现。
#[test]
fn capture_response_required_fields_appear_in_both_rust_and_typescript() {
    let schema_raw = std::fs::read_to_string("../../bridge-protocol/schema.json")
        .expect("bridge-protocol/schema.json must exist");
    let schema: Value = serde_json::from_str(&schema_raw).expect("schema.json must be valid JSON");

    let rust_source = std::fs::read_to_string("src/bridge.rs").expect("src/bridge.rs must exist");
    let ts_source = std::fs::read_to_string("../../extension/src/bridge.ts")
        .expect("extension/src/bridge.ts must exist");

    assert_required_fields_appear_in_both(
        &schema,
        "CaptureResponse",
        &rust_source,
        "crates/ctxrelay-cli/src/bridge.rs",
        &ts_source,
        "extension/src/bridge.ts",
    );
}
