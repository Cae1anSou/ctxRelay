//! `ctxrelay-cli` 的库目标:目前只导出 `bridge` 模块给集成测试用,主要逻辑仍在
//! `main.rs` 的二进制里(CLI 是薄封装,不需要把 clap 那套也搬进库里)。

pub mod bridge;
