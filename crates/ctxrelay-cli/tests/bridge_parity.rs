use serde_json::Value;

/// `bridge-protocol/schema.json` 是 CLI ↔ 扩展契约的唯一权威来源(见架构文档
/// §10.1),但 Rust 端(`bridge.rs`)和 TS 端(`extension/src/background.ts`)目前
/// 都是手写的投影,没有代码生成兜底——这意味着任何一边改了字段名而没改另一边,
/// `cargo test` 和 `npm run build` 各自都不会报错,只会在真机跑起来时才炸(架构
/// 文档 §1 明确点名要避免的那种"静默失败")。
///
/// 这条测试不是完整的类型系统一致性证明(没有真的解析 TS AST),但足够便宜、足够
/// 有效地抓住最常见的漂移方式:直接把 schema.json 里 `CaptureRequest.required`
/// 列出的每个字段名,当作字符串去两边源码里找——改名字的话,新名字不会同时出现
/// 在三处,这条测试就会红。
#[test]
fn capture_request_required_fields_appear_in_both_rust_and_typescript() {
    let schema_raw = std::fs::read_to_string("../../bridge-protocol/schema.json")
        .expect("bridge-protocol/schema.json must exist");
    let schema: Value = serde_json::from_str(&schema_raw).expect("schema.json must be valid JSON");

    let required = schema["definitions"]["CaptureRequest"]["required"]
        .as_array()
        .expect("CaptureRequest.required must be an array");

    let rust_source = std::fs::read_to_string("src/bridge.rs").expect("src/bridge.rs must exist");
    let ts_source = std::fs::read_to_string("../../extension/src/background.ts")
        .expect("extension/src/background.ts must exist");

    for field in required {
        let field_name = field
            .as_str()
            .expect("required field entries must be strings");
        assert!(
            rust_source.contains(field_name),
            "schema.json requires {field_name:?} but it's missing from crates/ctxrelay-cli/src/bridge.rs \
             — CaptureRequest's Rust projection has drifted from the schema"
        );
        assert!(
            ts_source.contains(field_name),
            "schema.json requires {field_name:?} but it's missing from extension/src/background.ts \
             — CaptureRequest's TypeScript projection has drifted from the schema"
        );
    }
}
