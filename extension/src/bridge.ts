interface CaptureRequest {
  version: "1";
  token: string;
  frontend_id: string;
  captured_at: string;
  snapshot: unknown;
}

interface CaptureResponse {
  version: "1";
  status: "ok" | "error";
  message?: string;
}

/**
 * 把已经拿到手的 snapshot 打包成 bridge-protocol/schema.json 定义的
 * CaptureRequest,POST 给本地 ctxrelay listen,并把结果反映到工具栏 badge 上。
 * 这一步不关心 snapshot 内部长什么样(不同 frontendId 的 shape 完全不同),只负责
 * 传输和信号上报——这正是它能对所有抓取源通用的原因。
 */
export async function sendCapture(
  tabId: number,
  frontendId: string,
  token: string,
  port: number,
  snapshot: unknown
): Promise<void> {
  const captureRequest: CaptureRequest = {
    version: "1",
    token,
    frontend_id: frontendId,
    captured_at: new Date().toISOString(),
    snapshot,
  };

  try {
    const postRes = await fetch(`http://127.0.0.1:${port}/capture`, {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-CtxRelay-Token": token },
      body: JSON.stringify(captureRequest),
    });
    // 只看 postRes.ok 曾经是个 bug:ctxrelay listen 以前不管管线是否真的成功
    // 都回 HTTP 200,只在 body 里把 status 标成 "error"——所以这里必须解析
    // body、看 status 字段,而不是只信 HTTP 状态码。服务端现在已经改成失败回
    // 非 2xx 了,但两条信号都查一遍(postRes.ok 且 status === "ok")才算真正
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
