import { execFile } from "node:child_process";
import { readFileSync } from "node:fs";
import { promisify } from "node:util";
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";
import { canonicalKeys, incomingBatch, MESSAGE_TYPE, pollDelay, renderBatch } from "./inbox.mjs";

const run = promisify(execFile);

export default function (pi: ExtensionAPI) {
  let context: ExtensionContext | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let generation = 0;
  let lastActivity = Date.now();
  let reading = false;
  let controller: AbortController | undefined;
  const queued = new Set<string>();

  function stop() {
    generation++;
    if (timer) clearTimeout(timer);
    controller?.abort();
    context = undefined;
    queued.clear();
  }

  async function poll() {
    const ctx = context;
    const current = generation;
    if (!ctx) return;
    try {
      if (ctx.isIdle() && !ctx.hasPendingMessages() && !reading) {
        reading = true;
        controller = new AbortController();
        const { stdout } = await run("python3", ["/opt/demo/poll.py"], {
          signal: controller.signal, maxBuffer: 4 * 1024 * 1024,
        });
        if (current !== generation) return;
        const config = JSON.parse(readFileSync(`${process.env.HOME}/inbox.json`, "utf8"));
        const seen = canonicalKeys(ctx.sessionManager.getBranch());
        for (const key of queued) seen.add(key);
        const batch = incomingBatch(JSON.parse(stdout), config, seen);
        if (batch.length) {
          for (const { key } of batch) queued.add(key);
          pi.sendMessage({ customType: MESSAGE_TYPE, content: renderBatch(batch), display: true,
            details: { keys: batch.map(({ key }) => key) } }, { triggerTurn: true });
          lastActivity = Date.now();
        }
        ctx.ui.setStatus("demo-inbox", "Inbox polling");
      }
    } catch {
      if (current === generation) ctx.ui.setStatus("demo-inbox", "Inbox read failed; check mailbox setup");
    } finally {
      if (current === generation) {
        reading = false;
        timer = setTimeout(poll, pollDelay(process.env, { idle: ctx.isIdle(), lastActivity }));
      }
    }
  }

  pi.on("session_start", async (_event, ctx) => {
    stop();
    reading = false;
    context = ctx;
    lastActivity = Date.now();
    timer = setTimeout(poll, pollDelay(process.env, { idle: ctx.isIdle(), lastActivity }));
  });
  pi.on("agent_start", async () => { lastActivity = Date.now(); });
  pi.on("agent_end", async () => {
    lastActivity = Date.now();
    // Durable custom messages, not queued delivery, determine restart deduplication.
    queued.clear();
  });
  pi.on("session_shutdown", async () => { stop(); });
}
