const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const appEl = document.getElementById("app");
const listEl = document.getElementById("provider-list");
const primaryBtn = document.getElementById("primary-btn");
const secondaryBtn = document.getElementById("secondary-btn");
const errorEl = document.getElementById("error");
const pickerEl = document.getElementById("picker");
const waitingEl = document.getElementById("waiting");
const waitingTitle = document.getElementById("waiting-title");
const waitingBody = document.getElementById("waiting-body");
const waitingHint = document.getElementById("waiting-hint");
const waitingError = document.getElementById("waiting-error");
const cancelWaitBtn = document.getElementById("cancel-wait-btn");
const authSuccessEl = document.getElementById("auth-success");
const successContinueBtn = document.getElementById("success-continue-btn");
const chatEl = document.getElementById("chat");
const chatMeta = document.getElementById("chat-meta");
const modelChipLabel = document.getElementById("model-chip-label");
const transcriptEl = document.getElementById("transcript");
const composer = document.getElementById("composer");
const composerInput = document.getElementById("composer-input");
const sendBtn = document.getElementById("send-btn");
const changeProviderBtn = document.getElementById("change-provider-btn");

/** @type {Array<any>} */
let providers = [];
let selectedId = null;
let pollTimer = null;
let pollTicks = 0;
let sending = false;
/** @type {HTMLElement | null} */
let activeAssistant = null;

const POLL_MS = 1500;
const POLL_MAX_TICKS = 120;

function showError(message) {
  if (!message) {
    errorEl.hidden = true;
    errorEl.textContent = "";
    return;
  }
  errorEl.hidden = false;
  errorEl.textContent = message;
}

function showWaitingError(message) {
  if (!message) {
    waitingError.hidden = true;
    waitingError.textContent = "";
    return;
  }
  waitingError.hidden = false;
  waitingError.textContent = message;
}

function selected() {
  return providers.find((p) => p.id === selectedId) || null;
}

function showScreen(name) {
  const auth = name !== "chat";
  appEl.classList.toggle("mode-auth", auth);
  appEl.classList.toggle("mode-chat", !auth);

  pickerEl.hidden = name !== "picker";
  waitingEl.hidden = name !== "waiting";
  authSuccessEl.hidden = name !== "auth-success";
  chatEl.hidden = name !== "chat";
}

function updateActions() {
  const row = selected();
  if (!row) {
    primaryBtn.disabled = true;
    secondaryBtn.hidden = true;
    return;
  }

  const ready = row.statusKind === "ready" || row.statusKind === "unknown";
  primaryBtn.disabled = !ready;
  primaryBtn.textContent = "Continue";

  if (row.canConnect) {
    secondaryBtn.hidden = false;
    secondaryBtn.textContent = row.statusKind === "ready" ? "Reconnect" : "Connect";
  } else {
    secondaryBtn.hidden = true;
  }
}

function render() {
  listEl.innerHTML = "";
  for (const p of providers) {
    const li = document.createElement("li");
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "provider" + (p.id === selectedId ? " selected" : "");
    btn.setAttribute("role", "option");
    btn.setAttribute("aria-selected", p.id === selectedId ? "true" : "false");

    const detail =
      p.statusKind === "ready"
        ? p.method
        : p.statusKind === "unknown"
          ? shortenUnknown(p.detail)
          : p.detail;

    btn.innerHTML = `
      <span class="dot ${p.statusKind}" aria-hidden="true"></span>
      <span class="provider-copy">
        <div class="provider-name">${escapeHtml(p.label)}</div>
        <div class="provider-detail">${escapeHtml(detail)}</div>
      </span>
      <span class="status ${p.statusKind}">${escapeHtml(p.statusLabel)}</span>
    `;

    btn.addEventListener("click", () => {
      selectedId = p.id;
      render();
    });

    li.appendChild(btn);
    listEl.appendChild(li);
  }
  updateActions();
}

function shortenUnknown(detail) {
  if (detail.toLowerCase().includes("outside a readable file")) {
    return "Installed — session stored outside a readable file";
  }
  return detail;
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

async function loadProviders({ preferSelection } = {}) {
  providers = await invoke("list_providers");
  if (preferSelection && providers.some((p) => p.id === preferSelection)) {
    selectedId = preferSelection;
  } else if (!selectedId || !providers.some((p) => p.id === selectedId)) {
    const ready = providers.find((p) => p.statusKind === "ready");
    selectedId = (ready || providers[0] || {}).id || null;
  }
  render();
  return providers;
}

function stopPolling() {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
  pollTicks = 0;
}

function startWaitingPoll() {
  stopPolling();
  pollTicks = 0;
  waitingHint.textContent = "Waiting for browser sign-in…";
  showWaitingError("");

  pollTimer = setInterval(async () => {
    pollTicks += 1;
    try {
      const rows = await loadProviders({ preferSelection: selectedId });
      const row = rows.find((p) => p.id === selectedId);
      if (row && row.statusKind === "ready") {
        stopPolling();
        showScreen("auth-success");
        return;
      }
    } catch (_) {}

    if (pollTicks >= POLL_MAX_TICKS) {
      stopPolling();
      waitingHint.textContent = "Still waiting";
      showWaitingError(
        "Still waiting — complete sign-in in the browser, or Cancel."
      );
    }
  }, POLL_MS);
}

function scrollTranscript() {
  transcriptEl.scrollTop = transcriptEl.scrollHeight;
}

function appendUser(text) {
  const el = document.createElement("div");
  el.className = "msg user";
  el.innerHTML = `<div class="msg-role">You</div><div class="msg-body"></div>`;
  el.querySelector(".msg-body").textContent = text;
  transcriptEl.appendChild(el);
  scrollTranscript();
}

function ensureAssistant() {
  if (activeAssistant) return activeAssistant;
  const el = document.createElement("div");
  el.className = "msg assistant streaming";
  el.innerHTML = `
    <div class="msg-tools"></div>
    <div class="msg-body"></div>
    <div class="msg-thinking" hidden></div>
    <div class="msg-error" hidden></div>
  `;
  transcriptEl.appendChild(el);
  activeAssistant = el;
  scrollTranscript();
  return el;
}

function syncSendEnabled() {
  const hasText = composerInput.value.trim().length > 0;
  sendBtn.disabled = sending || !hasText;
  sendBtn.classList.toggle("busy", sending);
}

function setSending(on) {
  sending = on;
  composerInput.disabled = on;
  syncSendEnabled();
}

function handleChatEvent(payload) {
  switch (payload.kind) {
    case "user":
      appendUser(payload.text);
      activeAssistant = null;
      break;
    case "text_delta": {
      const el = ensureAssistant();
      const body = el.querySelector(".msg-body");
      body.textContent += payload.text;
      scrollTranscript();
      break;
    }
    case "thinking_delta": {
      const el = ensureAssistant();
      const think = el.querySelector(".msg-thinking");
      think.hidden = false;
      think.textContent += payload.text;
      scrollTranscript();
      break;
    }
    case "tool_call": {
      const el = ensureAssistant();
      const tools = el.querySelector(".msg-tools");
      const row = document.createElement("div");
      row.className = "msg-tool";
      row.textContent = payload.name;
      tools.appendChild(row);
      scrollTranscript();
      break;
    }
    case "done":
      if (activeAssistant) activeAssistant.classList.remove("streaming");
      activeAssistant = null;
      setSending(false);
      composerInput.focus();
      break;
    case "error": {
      const el = ensureAssistant();
      el.classList.remove("streaming");
      const err = el.querySelector(".msg-error");
      err.hidden = false;
      err.textContent = payload.message;
      activeAssistant = null;
      setSending(false);
      scrollTranscript();
      break;
    }
  }
}

async function enterChat(info) {
  stopPolling();
  transcriptEl.innerHTML = "";
  activeAssistant = null;
  chatMeta.textContent = `${info.label} · Local`;
  if (modelChipLabel) {
    modelChipLabel.textContent = info.model;
  }
  showScreen("chat");
  setSending(false);
  composerInput.focus();
}

async function goContinue() {
  const row = selected();
  if (!row) return;
  showError("");
  primaryBtn.disabled = true;
  try {
    const info = await invoke("start_session", { id: row.id });
    await enterChat(info);
  } catch (err) {
    showScreen("picker");
    showError(String(err));
    updateActions();
  }
}

secondaryBtn.addEventListener("click", async () => {
  const row = selected();
  if (!row) return;
  showError("");
  try {
    const started = await invoke("start_login", { id: row.id });
    waitingTitle.textContent = started.browserTitle;
    waitingBody.textContent = started.browserBody;
    showScreen("waiting");
    startWaitingPoll();
  } catch (err) {
    showError(String(err));
  }
});

cancelWaitBtn.addEventListener("click", async () => {
  stopPolling();
  showWaitingError("");
  showScreen("picker");
  await loadProviders({ preferSelection: selectedId });
});

primaryBtn.addEventListener("click", () => goContinue());
successContinueBtn.addEventListener("click", () => goContinue());

changeProviderBtn.addEventListener("click", async () => {
  try {
    await invoke("end_session");
  } catch (_) {}
  transcriptEl.innerHTML = "";
  activeAssistant = null;
  showScreen("picker");
  await loadProviders({ preferSelection: selectedId });
});

composer.addEventListener("submit", async (e) => {
  e.preventDefault();
  if (sending) return;
  const text = composerInput.value.trim();
  if (!text) return;
  composerInput.value = "";
  autosize();
  setSending(true);
  activeAssistant = null;
  try {
    await invoke("send_message", { text });
  } catch (err) {
    // Error event usually already emitted; keep UI unlocked.
    setSending(false);
    if (!String(err).includes("already generating")) {
      handleChatEvent({ kind: "error", message: String(err) });
    }
  }
});

composerInput.addEventListener("keydown", (e) => {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    composer.requestSubmit();
  }
});

function autosize() {
  composerInput.style.height = "auto";
  composerInput.style.height = Math.min(composerInput.scrollHeight, 180) + "px";
  syncSendEnabled();
}
composerInput.addEventListener("input", autosize);
syncSendEnabled();

window.addEventListener("focus", () => {
  if (!waitingEl.hidden) {
    loadProviders({ preferSelection: selectedId })
      .then((rows) => {
        const row = rows.find((p) => p.id === selectedId);
        if (row && row.statusKind === "ready") {
          stopPolling();
          showScreen("auth-success");
        }
      })
      .catch(() => {});
    return;
  }
  if (!pickerEl.hidden) {
    loadProviders({ preferSelection: selectedId }).catch(() => {});
  }
});

listen("chat-event", (event) => {
  handleChatEvent(event.payload);
});

loadProviders().catch((err) => {
  showError(String(err));
});
