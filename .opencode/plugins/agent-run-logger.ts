import * as path from "node:path";
import { spawn } from "node:child_process";
import type { PluginInput, Hooks } from "@opencode-ai/plugin";

// Session-bounded deduplication state: Map<sessionID, lastCompletedMessageID>
const processedMessages = new Map<string, string>();
const inFlightMessages = new Set<string>();

function safeLog(input: PluginInput, level: "warn", message: string, extra?: Record<string, unknown>) {
  try {
    input.client.app.log({
      body: {
        service: "agent-run-logger",
        level,
        message,
        extra,
      },
    });
  } catch {
    // Silently ignore structured logging failures
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
          safeLog(input, "warn", "Could not retrieve session messages", { sessionID });
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
        const payload = {
          event: "session.idle",
          sessionID,
          messageID,
          workingDirectory: input.worktree,
          timestamp: new Date().toISOString(),
          summary,
        };

        // 8. Use current plugin worktree to resolve script path
        const scriptPath = path.join(input.worktree, "scripts", "agents", "notify-turn-complete.sh");

        // Guard flag to prevent double-reporting when both error and exit fire
        let settled = false;

        const finalize = () => {
          if (settled) return;
          settled = true;
          inFlightMessages.delete(dedupKey);
        };

        // 9. Invoke existing script using injection-safe process API
        let child: ReturnType<typeof spawn>;
        try {
          child = spawn("bash", [scriptPath, "opencode-session-idle"], {
            cwd: input.worktree,
            env: process.env,
            stdio: ["pipe", "ignore", "ignore"],
          });
        } catch (err: unknown) {
          finalize();
          safeLog(input, "warn", "Failed to spawn notify script", {
            sessionID,
            messageID,
            errorName: err instanceof Error ? err.name : "UnknownError",
          });
          return;
        }

        child.on("error", (err: Error) => {
          finalize();
          safeLog(input, "warn", "Notify script error", {
            sessionID,
            messageID,
            errorName: err.name,
          });
        });

        child.on("exit", (code) => {
          finalize();
          if (code !== 0) {
            safeLog(input, "warn", "Notify script exited nonzero", {
              sessionID,
              messageID,
              exitCode: code ?? -1,
            });
          } else {
            // Mark as successfully processed for this session
            processedMessages.set(sessionID, messageID);
          }
        });

        // 10. Write the structured JSON payload to standard input
        try {
          child.stdin.write(JSON.stringify(payload));
          child.stdin.end();
        } catch (err: unknown) {
          finalize();
          safeLog(input, "warn", "Failed to write to notify script stdin", {
            sessionID,
            messageID,
            errorName: err instanceof Error ? err.name : "UnknownError",
          });
        }

      } catch (err: unknown) {
        // Never allow telemetry failure to interrupt or fail the OpenCode session
        safeLog(input, "warn", "Error in session.idle handler", {
          errorName: err instanceof Error ? err.name : "UnknownError",
        });
      }
    },
  };
}
