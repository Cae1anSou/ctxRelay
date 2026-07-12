import { resolveSource } from "./sources/registry.js";
import { sendCapture } from "./bridge.js";

async function captureAndSend(tabId: number, tabUrl: string): Promise<void> {
  const source = resolveSource(tabUrl);
  if (!source) {
    await chrome.action.setBadgeText({ text: "N/A", tabId });
    return;
  }

  const stored = await chrome.storage.local.get(["ctxrelayToken", "ctxrelayPort"]);
  const token = typeof stored.ctxrelayToken === "string" ? stored.ctxrelayToken : "";
  const port = typeof stored.ctxrelayPort === "number" ? stored.ctxrelayPort : 47651;

  if (!token) {
    await chrome.action.setBadgeText({ text: "CFG", tabId });
    return;
  }

  // 整个抓取阶段包进 try/catch——不只是最后一步 POST。`res.ok` 只覆盖"服务器
  // 返回了非 2xx",覆盖不了网络本身抛异常的情况(比如断网、DNS 失败),那种异常
  // 如果不接住会变成 service worker 里一个没人处理的 rejection,用户在图标上
  // 什么反馈都看不到,跟"什么都没做"没法区分。
  let snapshot: unknown | null;
  try {
    snapshot = await source.capture(tabUrl);
  } catch {
    await chrome.action.setBadgeText({ text: "ERR", tabId });
    return;
  }
  if (snapshot === null) {
    await chrome.action.setBadgeText({ text: "ERR", tabId });
    return;
  }

  await sendCapture(tabId, source.frontendId, token, port, snapshot);
}

chrome.action.onClicked.addListener((tab) => {
  if (tab.id === undefined || !tab.url) {
    return;
  }
  void captureAndSend(tab.id, tab.url);
});
