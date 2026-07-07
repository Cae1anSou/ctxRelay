interface CaptureRequest {
  version: "1";
  token: string;
  conversation_id: string;
  org_id: string;
  captured_at: string;
  snapshot: unknown;
}

interface Organization {
  uuid: string;
}

const CHAT_URL_PATTERN = /^https:\/\/claude\.ai\/chat\/([0-9a-f-]{36})/;

async function findSnapshotAndOrg(
  conversationId: string
): Promise<{ snapshot: unknown; orgId: string } | null> {
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
      const snapshot = await res.json();
      return { snapshot, orgId: org.uuid };
    }
  }
  return null;
}

async function captureAndSend(tabId: number, tabUrl: string): Promise<void> {
  const match = tabUrl.match(CHAT_URL_PATTERN);
  if (!match) {
    await chrome.action.setBadgeText({ text: "N/A", tabId });
    return;
  }
  const conversationId = match[1];

  const stored = await chrome.storage.local.get(["ctxrelayToken", "ctxrelayPort"]);
  const token = typeof stored.ctxrelayToken === "string" ? stored.ctxrelayToken : "";
  const port = typeof stored.ctxrelayPort === "number" ? stored.ctxrelayPort : 47651;

  if (!token) {
    await chrome.action.setBadgeText({ text: "CFG", tabId });
    return;
  }

  const found = await findSnapshotAndOrg(conversationId);
  if (!found) {
    await chrome.action.setBadgeText({ text: "ERR", tabId });
    return;
  }

  const captureRequest: CaptureRequest = {
    version: "1",
    token,
    conversation_id: conversationId,
    org_id: found.orgId,
    captured_at: new Date().toISOString(),
    snapshot: found.snapshot,
  };

  try {
    const postRes = await fetch(`http://127.0.0.1:${port}/capture`, {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-CtxRelay-Token": token },
      body: JSON.stringify(captureRequest),
    });
    await chrome.action.setBadgeText({ text: postRes.ok ? "OK" : "ERR", tabId });
  } catch {
    // ctxrelay listen 大概率还没起,或者端口不对——用户需要先在终端跑 listen。
    await chrome.action.setBadgeText({ text: "N/L", tabId });
  }
}

chrome.action.onClicked.addListener((tab) => {
  if (tab.id === undefined || !tab.url) {
    return;
  }
  void captureAndSend(tab.id, tab.url);
});
