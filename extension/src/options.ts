const tokenInput = document.getElementById("token") as HTMLInputElement;
const portInput = document.getElementById("port") as HTMLInputElement;
const saveButton = document.getElementById("save") as HTMLButtonElement;
const statusEl = document.getElementById("status") as HTMLParagraphElement;

async function load(): Promise<void> {
  const stored = await chrome.storage.local.get(["ctxrelayToken", "ctxrelayPort"]);
  if (typeof stored.ctxrelayToken === "string") {
    tokenInput.value = stored.ctxrelayToken;
  }
  if (typeof stored.ctxrelayPort === "number") {
    portInput.value = String(stored.ctxrelayPort);
  }
}

saveButton.addEventListener("click", async () => {
  const token = tokenInput.value.trim();
  const port = parseInt(portInput.value, 10) || 47651;
  await chrome.storage.local.set({ ctxrelayToken: token, ctxrelayPort: port });
  statusEl.textContent = "已保存。";
});

void load();
