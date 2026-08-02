import * as path from "node:path";
import { spawn, type ChildProcess } from "node:child_process";
import type { PluginInput, Hooks } from "@opencode-ai/plugin";

// Session-bounded deduplication state: Map<sessionID, lastCompletedMessageID>
const processedMessages = new Map<string, string>();
const inFlightMessages = new Set<string>();

async function safeLog(
  input: PluginInput,
  level: "warn",
  message: string,
  extra?: Record<string, unknown>,
): Promise<void> {
  try {
    await input.client.app.log({
      body: {
        service: "agent-run-logger",
        level,
        message,
        extra,
      },
    });
  } catch {
    // Structured logging must never affect the OpenCode session.
  }
}

// Dependencies for the child-process lifecycle, injectable for testing.
export type LifecycleDeps = {
  spawnSync: typeof spawn;
  safeLogSync: (message: string, extra?: Record<string, unknown>) => void;
  processedMessages: Map<string, string>;
  inFlightMessages: Set<string>;
};

// Execute the child-process lifecycle for a single telemetry record.
// Returns a promise that resolves when the process has fully closed.
// Exported for testing only; not part of the public plugin contract.
export async function executeLifecycle(
  sessionID: string,
  messageID: string,
  payload: string,
  scriptPath: string,
  cwd: string,
  deps: LifecycleDeps,
): Promise<void> {
  const dedupKey = `${sessionID}:${messageID}`;
  let settled = false;

  const settle = (): boolean => {
    if (settled) {
      return false;
    }
    settled = true;
    deps.inFlightMessages.delete(dedupKey);
    return true;
  };

  // Wrap safeLogSync so synchronous throws never escape callbacks.
  const log = (message: string, extra?: Record<string, unknown>) => {
    try {
      deps.safeLogSync(message, extra);
    } catch {
      // Logging failure must never interrupt the session.
    }
  };

  let child: ChildProcess;
  try {
    child = deps.spawnSync("bash", [scriptPath, "opencode-session-idle"], {
      cwd,
      env: process.env,
      stdio: ["pipe", "ignore", "ignore"],
    });
  } catch (err: unknown) {
    if (!settle()) {
      return;
    }
    log("Failed to spawn notify script", {
      sessionID,
      messageID,
      errorName: err instanceof Error ? err.name : "UnknownError",
    });
    return;
  }

  child.on("error", (err: Error) => {
    if (!settle()) {
      return;
    }
    log("Notify script error", {
      sessionID,
      messageID,
      errorName: err.name,
    });
  });

  child.stdin.on("error", (err: Error) => {
    if (!settle()) {
      return;
    }
    log("Notify script stdin error", {
      sessionID,
      messageID,
      errorName: err.name,
    });
  });

  child.on("close", (code, signal) => {
    if (!settle()) {
      return;
    }
    if (code === 0) {
      deps.processedMessages.set(sessionID, messageID);
      return;
    }
    log("Notify script did not complete successfully", {
      sessionID,
      messageID,
      exitCode: code ?? -1,
      signal: signal ?? undefined,
    });
  });

  try {
    child.stdin.write(payload);
    child.stdin.end();
  } catch (err: unknown) {
    if (!settle()) {
      return;
    }
    log("Notify script stdin write failed", {
      sessionID,
      messageID,
      errorName: err instanceof Error ? err.name : "UnknownError",
    });
    // Best-effort terminate the child without allowing errors to escape.
    try {
      child.kill();
    } catch {
      // Ignore termination errors.
    }
  }
}

export default async function agentRunLogger(input: PluginInput): Promise<Hooks> {
  return {
    event: async ({ event }) => {
      try {
        // 1. React only to session.idle
        if (event.type !== "session.idle") {
          return;
        }

        // 2. Obtain session ID from the documented event shape
        const sessionID = event.properties.sessionID;
        if (!sessionID) {
          return;
        }

        // 3. Use the SDK client to retrieve session messages
        const result = await input.client.session.messages({ path: { id: sessionID } });
        if (result.error || !result.data || !Array.isArray(result.data)) {
          await safeLog(input, "warn", "Could not retrieve session messages", { sessionID });
          return;
        }

        // 4. Locate the most recent completed assistant message
        let latestAssistantMessage: typeof result.data[0] | null = null;
        for (let i = result.data.length - 1; i >= 0; i--) {
          const msg = result.data[i];
          if (msg.info?.role === "assistant" && msg.info?.time?.completed) {
            latestAssistantMessage = msg;
            break;
          }
        }

        if (!latestAssistantMessage) {
          return;
        }

        const messageID = latestAssistantMessage.info.id;
        if (!messageID) {
          return;
        }

        // 5. Prevent duplicate log records using session + message combo
        const dedupKey = `${sessionID}:${messageID}`;

        // If already in flight, do not launch a duplicate process
        if (inFlightMessages.has(dedupKey)) {
          return;
        }

        // If this message was already successfully processed, skip
        const lastCompleted = processedMessages.get(sessionID);
        if (lastCompleted === messageID) {
          return;
        }

        inFlightMessages.add(dedupKey);

        // 6. Extract only normal assistant text parts for the summary
        const parts = latestAssistantMessage.parts || [];
        const textParts = parts.filter(
          (p) => p.type === "text" && !p.synthetic && !p.ignored
        );

        // 7. Normalize whitespace
        const rawSummary = textParts.map((p) => p.text || "").join(" ");
        let summary = rawSummary.replace(/\s+/g, " ").trim() || "No text content";

        // Bound summary to ~2000 characters
        if (summary.length > 2000) {
          summary = summary.slice(0, 2000);
        }

        // Build the allow-listed payload (no complete messages, user prompts, reasoning, etc)
        const payloadObj = {
          event: "session.idle",
          sessionID,
          messageID,
          workingDirectory: input.worktree,
          timestamp: new Date().toISOString(),
          summary,
        };

        // 8. Use current plugin worktree to resolve script path
        const scriptPath = path.join(input.worktree, "scripts", "agents", "notify-turn-complete.sh");

        // 9. Execute child-process lifecycle
        const lifecycleDeps: LifecycleDeps = {
          spawnSync: spawn,
          safeLogSync: (msg, extra) => {
            void safeLog(input, "warn", msg, extra);
          },
          processedMessages,
          inFlightMessages,
        };

        await executeLifecycle(
          sessionID,
          messageID,
          JSON.stringify(payloadObj),
          scriptPath,
          input.worktree,
          lifecycleDeps,
        );
      } catch (err: unknown) {
        // Never allow telemetry failure to interrupt or fail the OpenCode session
        await safeLog(input, "warn", "Error in session.idle handler", {
          errorName: err instanceof Error ? err.name : "UnknownError",
        });
      }
    },
  };
}
