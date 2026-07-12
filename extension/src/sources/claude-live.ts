import type { CaptureSource } from "./types.js";

interface Organization {
  uuid: string;
}

const CHAT_URL_PATTERN = /^https:\/\/claude\.ai\/chat\/([0-9a-f-]{36})/;

async function findSnapshot(conversationId: string): Promise<unknown | null> {
  const orgsRes = await fetch("https://claude.ai/api/organizations", { credentials: "include" });
  if (!orgsRes.ok) {
    return null;
  }
  const orgs = (await orgsRes.json()) as Organization[];

  // 大多数账号只有一个组织,这里走一个快速路径,但仍然兼容多组织账号——
  // 逐个尝试,直到某个组织真的能看到这条对话(实测确认过这个接口对话不存在
  // 时返回非 200,不会抛异常,只是 res.ok 为 false)。
  for (const org of orgs) {
    const url =
      `https://claude.ai/api/organizations/${org.uuid}/chat_conversations/${conversationId}` +
      `?tree=True&rendering_mode=messages&render_all_tools=true&consistency=strong`;
    const res = await fetch(url, { credentials: "include" });
    if (res.ok) {
      return await res.json();
    }
  }
  return null;
}

/**
 * claude.ai 的抓取源:认领 /chat/<uuid> 页面,调认证态内部 API 拿整棵对话树。
 * frontendId 必须等于 fe-claude-live 这个 crate 的 Parse::id()
 * (crates/fe-claude-live/src/parse.rs)——两侧靠这个字符串配对,如同 Rust 内部
 * Acquire/Parse 靠 id() 配对一样(见 crates/ctxrelay-core/src/registry.rs)。
 */
export const claudeLiveSource: CaptureSource = {
  frontendId: "fe-claude-live",
  matches: (url) => CHAT_URL_PATTERN.test(url),
  capture: async (url) => {
    const match = url.match(CHAT_URL_PATTERN);
    if (!match) {
      return null;
    }
    return findSnapshot(match[1]);
  },
};
