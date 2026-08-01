import * as path from "node:path";
import { spawn } from "node:child_process";
import type { PluginInput, Hooks } from "@opencode-ai/plugin";

// In-memory map to prevent duplicate logging for the same assistant message
const processedMessages = new Set<string>();

export default async function agentRunLogger(input: PluginInput): Promise<Hooks> {
  return {
    event: async ({ event }) => {
      try {
        // 1. React only to session.idle
        if (event.type !== "session.idle") {
          return;
        }

        // 2. Obtain session ID from the documented event shape
        // Using `any` cast to safely extract properties from the union type
        const sessionID = (event as any).properties?.sessionID;
        if (!sessionID) {
          return;
        }

        // 3. Use the SDK client to retrieve session messages
        const result = await input.client.messages({ path: { id: sessionID } });
        if (result.error || !result.data || !Array.isArray(result.data)) {
          console.warn("[agent-run-logger] Could not retrieve session messages");
          return;
        }

        // 4. Locate the most recent completed assistant message
        let latestAssistantMessage: any = null;
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

        // 10. Prevent duplicate log records using session + message combo
        // 11. Allow later assistant turns in the same session to produce new records
        const dedupKey = `${sessionID}:${messageID}`;
        if (processedMessages.has(dedupKey)) {
          return;
        }
        processedMessages.add(dedupKey);

        // 5. Extract only normal assistant text parts for the summary
        // 6. Exclude reasoning, tool arguments, tool output, file contents, etc.
        const parts = latestAssistantMessage.parts || [];
        const textParts = parts.filter(
          (p: any) => p.type === "text" && !p.synthetic && !p.ignored
        );

        // 7. Normalize whitespace
        const rawSummary = textParts.map((p: any) => p.text || "").join(" ");
        const summary = rawSummary.replace(/\s+/g, " ").trim() || "No text content";

        // Build the allow-listed payload (no complete messages, user prompts, reasoning, etc)
        const payload = {
          event: "session.idle",
          sessionID,
          messageID,
          workingDirectory: input.worktree,
          timestamp: new Date().toISOString(),
        };

        // 9. Use current plugin worktree to resolve script path
        const scriptPath = path.join(input.worktree, "scripts", "agents", "notify-turn-complete.sh");

        // 8. Invoke existing script using injection-safe process API
        const child = spawn("bash", [scriptPath, "session.idle", summary], {
          cwd: input.worktree,
          env: process.env,
          stdio: ["pipe", "ignore", "ignore"],
        });

        // 13. Catch retrieval, parsing, spawn, and script-exit failures
        // 14. Report failures through structured app logging at warning level
        child.on("error", (err: Error) => {
          console.warn("[agent-run-logger] Failed to spawn notify script:", err.message);
        });

        child.on("exit", (code) => {
          if (code !== null && code !== 0) {
            console.warn(`[agent-run-logger] Notify script exited with code ${code}`);
          }
        });

        // Write the structured JSON payload to standard input
        child.stdin.write(JSON.stringify(payload));
        child.stdin.end();

      } catch (err: any) {
        // 12. Never allow telemetry failure to interrupt or fail the OpenCode session
        console.warn("[agent-run-logger] Error in session.idle handler:", err.message);
      }
    },
  };
}
