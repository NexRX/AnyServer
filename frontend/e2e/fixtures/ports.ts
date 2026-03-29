/**
 * Port allocation utility for parallel test execution.
 * Each worker gets a unique set of ports to avoid conflicts.
 */

const BASE_BACKEND_PORT = 4000;
const BASE_FRONTEND_PORT = 5000;
const PORT_OFFSET_PER_WORKER = 100; // Give each worker a large range to avoid collisions

export interface PortSet {
  backend: number;
  frontend: number;
}

/**
 * Get a unique set of ports for a test worker.
 * Worker IDs are typically 1-indexed (1, 2, 3, ...).
 */
export function getPortsForWorker(workerId: number): PortSet {
  const offset = (workerId - 1) * PORT_OFFSET_PER_WORKER;
  return {
    backend: BASE_BACKEND_PORT + offset,
    frontend: BASE_FRONTEND_PORT + offset,
  };
}

/**
 * Find an available port by attempting to bind to it.
 * Falls back to a range if the preferred port is taken.
 * Searches within the worker's allocated port range to avoid collisions.
 */
export async function findAvailablePort(
  preferredPort: number,
): Promise<number> {
  const net = await import("net");

  const isPortAvailable = (port: number): Promise<boolean> => {
    return new Promise((resolve) => {
      const server = net.createServer();
      server.once("error", () => resolve(false));
      server.once("listening", () => {
        server.close();
        resolve(true);
      });
      server.listen(port);
    });
  };

  // Try the preferred port first
  if (await isPortAvailable(preferredPort)) {
    return preferredPort;
  }

  // Try nearby ports within the worker's allocated range
  // Search up to PORT_OFFSET_PER_WORKER - 10 to stay within our allocation
  const maxOffset = PORT_OFFSET_PER_WORKER - 10;
  for (let offset = 1; offset <= maxOffset; offset++) {
    const port = preferredPort + offset;
    if (await isPortAvailable(port)) {
      return port;
    }
  }

  throw new Error(
    `Could not find available port near ${preferredPort} within allocated range`,
  );
}
