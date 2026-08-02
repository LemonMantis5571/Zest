import type { ChatEvent } from "./types";

/** Offline UI streaming demo — no gateway required. */
export async function runFixtureStream(
  onEvent: (event: ChatEvent) => void
): Promise<void> {
  const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
  const sessionId = "session-fixture";
  const threadId = "fixture";
  const turnId = "turn-fixture";
  const userId = "user-fixture";
  const assistantId = "assistant-fixture";

  const id = {
    session_id: sessionId,
    thread_id: threadId,
    turn_id: turnId,
  };

  onEvent({
    kind: "user",
    ...id,
    message_id: userId,
    text: "What's in README.md?",
  });
  await sleep(120);

  onEvent({
    kind: "assistant_start",
    ...id,
    message_id: assistantId,
  });
  await sleep(200);

  onEvent({
    kind: "thinking_delta",
    ...id,
    message_id: assistantId,
    text: "I'll read the project README first.",
  });
  await sleep(350);

  onEvent({
    kind: "tool_call_start",
    ...id,
    message_id: assistantId,
    name: "read_file",
    id: "tool_fixture_1",
  });
  await sleep(500);

  onEvent({
    kind: "tool_call_result",
    ...id,
    message_id: assistantId,
    name: "read_file",
    id: "tool_fixture_1",
    summary: "# Zest — Rust coding harness…",
    isError: false,
  });
  await sleep(200);

  const reply =
    "Zest is a Rust coding harness with a Tauri desktop shell. The chat UI streams tool calls and text over Tauri events.";
  for (const word of reply.split(/(?<=\s)/)) {
    onEvent({
      kind: "text_delta",
      ...id,
      message_id: assistantId,
      text: word,
    });
    await sleep(28);
  }

  onEvent({ kind: "done", ...id, message_id: assistantId });
}
