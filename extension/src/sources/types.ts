/**
 * 一个"抓取源"认领一类页面(按 URL 匹配),知道怎么在该应用已登录的认证态下
 * 把当前对话的原始数据弄到手。这一层只做"怎么拿到数据",不解释数据内容——
 * 内容解释是 Rust 侧对应 frontendId 的 Parse 实现的职责,见
 * `crates/ctxrelay-frontend/src/lib.rs` 里 Acquire/Parse 的拆分,以及
 * `docs/architecture.md` §10.2。新增一个应用 = 新增一个实现这个接口的模块 +
 * 在 registry.ts 里注册一行,不需要改 background.ts 或桥接协议本身。
 */
export interface CaptureSource {
  /** 必须等于 Rust 侧对应 frontend crate 的 Parse::id(),是桥接协议里 frontend_id 字段的值。 */
  readonly frontendId: string;
  /** 便宜的同步判断:这个 tab 的 URL 是否属于本抓取源。 */
  matches(url: string): boolean;
  /** 做该应用私有的网络调用,拿到原始 payload;不是本源能处理的场景返回 null。 */
  capture(url: string): Promise<unknown | null>;
}
