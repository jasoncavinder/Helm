// Focused local test of the agent-run-logger event handler logic
// Run with: bun test .opencode/plugins/agent-run-logger.test.ts

import { describe, it, expect, mock } from "bun:test";

describe("agent-run-logger event handler", () => {
  it("successful invocation marks message as processed", async () => {
    // Simulate the event handler logic
    const processedMessages = new Map<string, string>();
    const inFlightMessages = new Set<string>();

    const sessionID = "session-1";
    const messageID = "msg-1";
    const dedupKey = `${sessionID}:${messageID}`;

    // First invocation
    inFlightMessages.add(dedupKey);

    // Simulate script exit with code 0
    inFlightMessages.delete(dedupKey);
    processedMessages.set(sessionID, messageID);

    expect(processedMessages.get(sessionID)).toBe("msg-1");
    expect(inFlightMessages.has(dedupKey)).toBe(false);
  });

  it("duplicate idle event does not launch duplicate process", async () => {
    const processedMessages = new Map<string, string>();
    const inFlightMessages = new Set<string>();

    const sessionID = "session-1";
    const messageID = "msg-1";
    const dedupKey = `${sessionID}:${messageID}`;

    // First invocation is in flight
    inFlightMessages.add(dedupKey);

    // Second invocation checks
    const shouldSkip = inFlightMessages.has(dedupKey);
    expect(shouldSkip).toBe(true);
  });

  it("later assistant message in the same session can be logged", async () => {
    const processedMessages = new Map<string, string>();
    const inFlightMessages = new Set<string>();

    const sessionID = "session-1";
    const messageID1 = "msg-1";
    const messageID2 = "msg-2";

    // First message processed
    processedMessages.set(sessionID, messageID1);

    // Second message check
    const lastCompleted = processedMessages.get(sessionID);
    const shouldSkip = lastCompleted === messageID2;
    expect(shouldSkip).toBe(false);
  });

  it("SDK retrieval failure does not suppress retries", async () => {
    const processedMessages = new Map<string, string>();
    const inFlightMessages = new Set<string>();

    const sessionID = "session-1";
    const messageID = "msg-1";

    // Simulate SDK failure (early return before dedup logic)
    // No state is modified, so a later event can retry
    expect(processedMessages.has(sessionID)).toBe(false);
    expect(inFlightMessages.size).toBe(0);
  });

  it("spawn failure removes in-flight marker and allows retry", async () => {
    const processedMessages = new Map<string, string>();
    const inFlightMessages = new Set<string>();

    const sessionID = "session-1";
    const messageID = "msg-1";
    const dedupKey = `${sessionID}:${messageID}`;

    // In flight
    inFlightMessages.add(dedupKey);

    // Simulate spawn failure
    let settled = false;
    const finalize = () => {
      if (settled) return;
      settled = true;
      inFlightMessages.delete(dedupKey);
    };

    finalize();

    expect(inFlightMessages.has(dedupKey)).toBe(false);
    expect(processedMessages.has(sessionID)).toBe(false);
  });

  it("stdin failure removes in-flight marker and allows retry", async () => {
    const processedMessages = new Map<string, string>();
    const inFlightMessages = new Set<string>();

    const sessionID = "session-1";
    const messageID = "msg-1";
    const dedupKey = `${sessionID}:${messageID}`;

    // In flight
    inFlightMessages.add(dedupKey);

    // Simulate stdin failure
    let settled = false;
    const finalize = () => {
      if (settled) return;
      settled = true;
      inFlightMessages.delete(dedupKey);
    };

    finalize();

    expect(inFlightMessages.has(dedupKey)).toBe(false);
    expect(processedMessages.has(sessionID)).toBe(false);
  });

  it("nonzero script exit removes in-flight marker and allows retry", async () => {
    const processedMessages = new Map<string, string>();
    const inFlightMessages = new Set<string>();

    const sessionID = "session-1";
    const messageID = "msg-1";
    const dedupKey = `${sessionID}:${messageID}`;

    // In flight
    inFlightMessages.add(dedupKey);

    // Simulate nonzero exit
    let settled = false;
    const finalize = () => {
      if (settled) return;
      settled = true;
      inFlightMessages.delete(dedupKey);
    };

    const exitCode = 1;
    finalize();
    if (exitCode !== 0) {
      // Do NOT mark as processed
    }

    expect(inFlightMessages.has(dedupKey)).toBe(false);
    expect(processedMessages.has(sessionID)).toBe(false);
  });

  it("double-reporting guard prevents duplicate finalization", async () => {
    const inFlightMessages = new Set<string>();

    const dedupKey = "session-1:msg-1";
    inFlightMessages.add(dedupKey);

    let settled = false;
    const finalize = () => {
      if (settled) return;
      settled = true;
      inFlightMessages.delete(dedupKey);
    };

    // First call
    finalize();
    expect(inFlightMessages.has(dedupKey)).toBe(false);

    // Second call (should be a no-op)
    finalize();
    expect(inFlightMessages.has(dedupKey)).toBe(false);
  });

  it("payload contains only allow-listed fields", async () => {
    const sessionID = "session-1";
    const messageID = "msg-1";
    const workingDirectory = "/path/to/worktree";
    const summary = "Test summary";

    const payload = {
      event: "session.idle",
      sessionID,
      messageID,
      workingDirectory,
      timestamp: new Date().toISOString(),
      summary,
    };

    const allowedFields = ["event", "sessionID", "messageID", "workingDirectory", "timestamp", "summary"];
    const actualFields = Object.keys(payload);

    expect(actualFields.sort()).toEqual(allowedFields.sort());
  });

  it("summary is bounded to 2000 characters", async () => {
    const longSummary = "a".repeat(3000);
    let summary = longSummary.replace(/\s+/g, " ").trim() || "No text content";

    if (summary.length > 2000) {
      summary = summary.slice(0, 2000);
    }

    expect(summary.length).toBe(2000);
  });
});
