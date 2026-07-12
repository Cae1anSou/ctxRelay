import type { CaptureSource } from "./types.js";
import { claudeLiveSource } from "./claude-live.js";

/**
 * 抓取源注册表——新增支持的应用时,在这里加一行就够,镜像 Rust 侧
 * Registry::with_defaults() 的"加一行"约定(crates/ctxrelay-core/src/registry.rs)。
 */
const SOURCES: CaptureSource[] = [claudeLiveSource];

export function resolveSource(url: string): CaptureSource | undefined {
  return SOURCES.find((source) => source.matches(url));
}
