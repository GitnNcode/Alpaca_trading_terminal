import { spawn, ChildProcess } from "child_process";
import * as net from "net";
import * as path from "path";
import * as http from "http";

let proc: ChildProcess | null = null;
let port = 0;

function findFreePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.unref();
    srv.on("error", reject);
    srv.listen(0, "127.0.0.1", () => {
      const addr = srv.address();
      if (typeof addr === "object" && addr && "port" in addr) {
        const p = (addr as net.AddressInfo).port;
        srv.close(() => resolve(p));
      } else {
        reject(new Error("no port"));
      }
    });
  });
}

function waitForHealth(p: number, timeoutMs = 15000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve, reject) => {
    const tick = () => {
      const req = http.get({ host: "127.0.0.1", port: p, path: "/health", timeout: 1000 }, (res) => {
        if (res.statusCode === 200) {
          res.resume();
          resolve();
        } else {
          res.resume();
          retry();
        }
      });
      req.on("error", retry);
      req.on("timeout", () => { req.destroy(); retry(); });
    };
    const retry = () => {
      if (Date.now() > deadline) {
        reject(new Error("sidecar /health timed out"));
        return;
      }
      setTimeout(tick, 250);
    };
    tick();
  });
}

export async function startSidecar(): Promise<void> {
  if (proc) return;
  port = await findFreePort();
  const py = process.env.CCB_PYTHON || "python3";
  const script = path.join(__dirname, "..", "..", "python", "server.py");
  proc = spawn(py, [script, "--port", String(port)], {
    stdio: ["ignore", "pipe", "pipe"],
    env: { ...process.env, PYTHONUNBUFFERED: "1" },
  });
  proc.stdout?.on("data", (b) => process.stdout.write(`[ccb-py] ${b}`));
  proc.stderr?.on("data", (b) => process.stderr.write(`[ccb-py] ${b}`));
  proc.on("exit", (code) => {
    console.error(`[ccb] sidecar exited (${code})`);
    proc = null;
  });
  await waitForHealth(port);
  console.log(`[ccb] sidecar ready on :${port}`);
}

export function stopSidecar(): void {
  if (proc) {
    try { proc.kill("SIGTERM"); } catch { /* noop */ }
    proc = null;
  }
}

export function sidecarPort(): number {
  return port;
}
