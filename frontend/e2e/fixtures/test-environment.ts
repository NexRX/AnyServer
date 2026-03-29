/**
 * Test environment fixture for E2E tests.
 * Spawns a real backend binary and Vite dev server for each test worker.
 */

import { spawn, ChildProcess } from "child_process";
import { test as base, expect } from "@playwright/test";
import * as path from "path";
import * as fs from "fs";
import * as os from "os";
import { fileURLToPath } from "url";
import { findAvailablePort, getPortsForWorker, PortSet } from "./ports";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

export interface TestEnvironment {
  baseUrl: string;
  apiUrl: string;
  backendPort: number;
  frontendPort: number;
  adminToken: string;
  dataDir: string;
  cleanup: () => Promise<void>;
  /**
   * Kill the backend process (simulates a crash / server going down).
   * The frontend (Vite proxy) remains running so the browser page stays loaded.
   */
  killBackend: () => Promise<void>;
  /**
   * Restart the backend process on the same port and data directory.
   * Waits for the backend to become healthy before returning.
   */
  restartBackend: () => Promise<void>;
}

interface TestFixtures {
  testEnv: TestEnvironment;
}

let backendProcess: ChildProcess | null = null;
let frontendProcess: ChildProcess | null = null;
let tempDir: string | null = null;

/**
 * Wait for a server to be ready by polling a health endpoint.
 */
async function waitForServer(
  url: string,
  timeout = 30000,
  interval = 100,
): Promise<void> {
  const startTime = Date.now();
  while (Date.now() - startTime < timeout) {
    try {
      const response = await fetch(url, { method: "HEAD" });
      if (response.ok || response.status === 404) {
        return;
      }
    } catch (err) {
      // Server not ready yet
    }
    await new Promise((resolve) => setTimeout(resolve, interval));
  }
  throw new Error(`Server at ${url} did not become ready within ${timeout}ms`);
}

/**
 * Create admin user and return JWT token.
 */
async function setupAdmin(apiUrl: string): Promise<string> {
  const response = await fetch(`${apiUrl}/api/auth/setup`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      username: "admin",
      password: "Admin123",
    }),
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(`Setup admin failed: ${response.status} ${text}`);
  }

  const data = await response.json();
  return data.token;
}

/**
 * Spawn the backend binary with environment variables.
 */
async function spawnBackend(
  port: number,
  dataDir: string,
): Promise<ChildProcess> {
  const backendBinary = path.join(
    __dirname,
    "../../../backend/target/debug/anyserver",
  );

  if (!fs.existsSync(backendBinary)) {
    throw new Error(
      `Backend binary not found at ${backendBinary}. Run 'cd backend && cargo build' first.`,
    );
  }

  return new Promise((resolve, reject) => {
    const proc = spawn(backendBinary, [], {
      env: {
        ...process.env,
        ANYSERVER_DATA_DIR: dataDir,
        ANYSERVER_HTTP_PORT: port.toString(),
        ANYSERVER_SFTP_PORT: "0", // Disable SFTP to avoid port conflicts
        RUST_LOG: "anyserver=info,warn",
        // Don't set CORS origin - let backend default to "any" in dev mode
      },
      stdio: ["ignore", "pipe", "pipe"],
    });

    let hasStarted = false;
    let errorOutput = "";

    // Log backend output for debugging
    proc.stdout?.on("data", (data) => {
      const msg = data.toString().trim();
      if (msg) {
        console.log(`[Backend:${port}]`, msg);
        if (msg.includes("listening on")) {
          hasStarted = true;
        }
      }
    });

    proc.stderr?.on("data", (data) => {
      const msg = data.toString().trim();
      if (msg) {
        console.error(`[Backend:${port}]`, msg);
        errorOutput += msg + "\n";
      }
    });

    proc.on("error", (err) => {
      console.error(`Backend process error:`, err);
      reject(err);
    });

    proc.on("exit", (code, signal) => {
      if (code !== 0 && code !== null && !hasStarted) {
        console.error(`Backend exited with code ${code}, signal ${signal}`);
        reject(new Error(`Backend failed to start: ${errorOutput}`));
      }
    });

    // Give the process a moment to fail if port is taken
    setTimeout(() => {
      if (proc.exitCode === null) {
        resolve(proc);
      }
    }, 500);
  });
}

/**
 * Spawn the Vite dev server or preview server (for production builds).
 */
async function spawnFrontend(
  port: number,
  backendPort: number,
): Promise<ChildProcess> {
  const frontendDir = path.join(__dirname, "../..");
  const useProductionBuild = process.env.USE_PRODUCTION_BUILD === "true";

  // Use preview mode for production builds, dev mode otherwise
  const viteBin = path.join(frontendDir, "node_modules", ".bin", "vite");
  const viteArgs = useProductionBuild
    ? ["preview", "--port", port.toString(), "--strictPort"]
    : [
        "--config",
        "vite.config.test.ts",
        "--port",
        port.toString(),
        "--strictPort",
      ];

  if (useProductionBuild) {
    console.log(
      `[Worker] Using PRODUCTION build (vite preview) on port ${port}`,
    );
  }

  const proc = spawn(viteBin, viteArgs, {
    cwd: frontendDir,
    env: {
      ...process.env,
      VITE_API_PROXY_TARGET: `http://localhost:${backendPort}`,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });

  // Log frontend output for debugging
  proc.stdout?.on("data", (data) => {
    const msg = data.toString().trim();
    if (msg) console.log(`[Frontend:${port}]`, msg);
  });

  proc.stderr?.on("data", (data) => {
    const msg = data.toString().trim();
    if (msg) console.error(`[Frontend:${port}]`, msg);
  });

  proc.on("error", (err) => {
    console.error(`Frontend process error:`, err);
  });

  return proc;
}

/**
 * Kill a process and wait for it to exit.
 */
async function killProcess(proc: ChildProcess, name: string): Promise<void> {
  if (!proc.pid) return;

  return new Promise((resolve) => {
    const timeout = setTimeout(() => {
      console.warn(`${name} did not exit gracefully, force killing`);
      proc.kill("SIGKILL");
      resolve();
    }, 5000);

    proc.once("exit", () => {
      clearTimeout(timeout);
      resolve();
    });

    proc.kill("SIGTERM");
  });
}

/**
 * Remove directory recursively.
 */
function rimraf(dir: string): void {
  if (!fs.existsSync(dir)) return;
  fs.rmSync(dir, { recursive: true, force: true });
}

/**
 * Create a test environment for a worker.
 */
async function createTestEnvironment(
  workerId: number,
): Promise<TestEnvironment> {
  // Get unique ports for this worker
  const ports: PortSet = getPortsForWorker(workerId);

  // Try to find available ports with retries to handle race conditions
  let backendPort: number;
  let frontendPort: number;
  let retries = 3;

  while (retries > 0) {
    try {
      backendPort = await findAvailablePort(ports.backend + (3 - retries) * 3);
      frontendPort = await findAvailablePort(
        ports.frontend + (3 - retries) * 3,
      );
      break;
    } catch (err) {
      retries--;
      if (retries === 0) throw err;
      console.warn(`[Worker ${workerId}] Port allocation failed, retrying...`);
      await new Promise((resolve) => setTimeout(resolve, 200));
    }
  }

  // Create temp directory for this worker
  const dataDir = fs.mkdtempSync(
    path.join(os.tmpdir(), `anyserver-e2e-${workerId}-`),
  );
  tempDir = dataDir;

  console.log(
    `[Worker ${workerId}] Starting backend on port ${backendPort}, frontend on ${frontendPort}`,
  );
  console.log(`[Worker ${workerId}] Data directory: ${dataDir}`);

  // Start backend with retry on port conflict
  let backendStartRetries = 3;
  while (backendStartRetries > 0) {
    try {
      backendProcess = await spawnBackend(backendPort, dataDir);
      break;
    } catch (err) {
      backendStartRetries--;
      if (backendStartRetries === 0) {
        throw new Error(`Failed to start backend after retries: ${err}`);
      }
      console.warn(
        `[Worker ${workerId}] Backend start failed, finding new port...`,
      );
      backendPort = await findAvailablePort(backendPort + 1);
    }
  }

  const apiUrl = `http://localhost:${backendPort}`;

  // Wait for backend to be ready
  await waitForServer(apiUrl, 30000);
  console.log(`[Worker ${workerId}] Backend ready at ${apiUrl}`);

  // Create admin user
  const adminToken = await setupAdmin(apiUrl);
  console.log(`[Worker ${workerId}] Admin user created`);

  // Start frontend
  frontendProcess = await spawnFrontend(frontendPort, backendPort);
  const baseUrl = `http://localhost:${frontendPort}`;

  // Wait for frontend to be ready
  await waitForServer(baseUrl, 60000);
  console.log(`[Worker ${workerId}] Frontend ready at ${baseUrl}`);

  const doKillBackend = async () => {
    if (backendProcess) {
      console.log(`[Worker ${workerId}] Killing backend (simulating crash)`);
      await killProcess(backendProcess, "Backend");
      backendProcess = null;
    }
  };

  const doRestartBackend = async () => {
    // Kill first if still running
    if (backendProcess) {
      await doKillBackend();
    }

    console.log(
      `[Worker ${workerId}] Restarting backend on port ${backendPort!}`,
    );
    backendProcess = await spawnBackend(backendPort!, dataDir);
    await waitForServer(apiUrl, 30000);
    console.log(`[Worker ${workerId}] Backend restarted and healthy`);
  };

  const cleanup = async () => {
    console.log(`[Worker ${workerId}] Cleaning up test environment`);

    if (frontendProcess) {
      await killProcess(frontendProcess, "Frontend");
      frontendProcess = null;
    }

    if (backendProcess) {
      await killProcess(backendProcess, "Backend");
      backendProcess = null;
    }

    if (tempDir) {
      rimraf(tempDir);
      tempDir = null;
    }
  };

  return {
    baseUrl,
    apiUrl,
    backendPort: backendPort!,
    frontendPort: frontendPort!,
    adminToken,
    dataDir,
    cleanup,
    killBackend: doKillBackend,
    restartBackend: doRestartBackend,
  };
}

/**
 * Playwright test fixture with test environment.
 */
export const test = base.extend<TestFixtures>({
  testEnv: async ({}, use, testInfo) => {
    const workerId = testInfo.parallelIndex + 1;
    const env = await createTestEnvironment(workerId);

    try {
      await use(env);
    } finally {
      await env.cleanup();
    }
  },
});

export { expect };
