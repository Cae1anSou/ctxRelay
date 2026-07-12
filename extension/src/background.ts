interface CaptureRequest {
  version: "1";
  token: string;
  conversation_id: string;
  org_id: string;
  captured_at: string;
  snapshot: unknown;
}

interface CaptureResponse {
  version: "1";
  status: "ok" | "error";
  message?: string;
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

  // 整个抓取阶段(拿组织列表、拿对话内容)包进 try/catch——不只是最后一步 POST。
  // `res.ok` 只覆盖"服务器返回了非 2xx",覆盖不了网络本身抛异常的情况(比如
  // 断网、DNS 失败),那种异常如果不接住会变成 service worker 里一个没人处理的
  // rejection,用户在图标上什么反馈都看不到,跟"什么都没做"没法区分。
  let found: { snapshot: unknown; orgId: string } | null;
  try {
    found = await findSnapshotAndOrg(conversationId);
  } catch {
    await chrome.action.setBadgeText({ text: "ERR", tabId });
    return;
  }
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
    // 只看 `postRes.ok` 曾经是个 bug:`ctxrelay listen` 以前不管管线是否真的成功
    // 都回 HTTP 200,只在 body 里把 `status` 标成 `"error"`——所以这里必须解析
    // body、看 `status` 字段,而不是只信 HTTP 状态码。服务端现在已经改成失败回
    // 非 2xx 了,但两条信号都查一遍(`postRes.ok` 且 `status === "ok"`)才算真正
    // 对齐 bridge-protocol 的契约,不会因为以后哪边单独改回去又变成静默失败。
    let succeeded = false;
    try {
      const responseBody = (await postRes.json()) as CaptureResponse;
      succeeded = postRes.ok && responseBody.status === "ok";
    } catch {
      // 响应体不是合法 JSON,理论上不该发生,但别让"解析响应体"本身变成又一个
      // 没人处理的异常——退化成只看 HTTP 状态码。
      succeeded = postRes.ok;
    }
    await chrome.action.setBadgeText({ text: succeeded ? "OK" : "ERR", tabId });
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
