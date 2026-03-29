/**
 * WebSocket helper functions for E2E tests.
 * Provides utilities for connecting to server consoles and monitoring WebSocket messages.
 */

import type { WsMessage, LogLine } from "../../src/types/bindings";

export interface ConsoleMessage {
  type: string;
  data: unknown;
}

export interface ConsoleConnection {
  ws: WebSocket;
  messages: WsMessage[];
  logs: LogLine[];
  close: () => void;
  waitForLog: (predicate: (log: LogLine) => boolean, timeout?: number) => Promise<LogLine>;
  waitForStatus: (status: string, timeout?: number) => Promise<void>;
}

/**
 * Connect to a server's WebSocket console.
 */
export async function connectToConsole(
  apiUrl: string,
  serverId: string,
  token: string
): Promise<ConsoleConnection> {
  const wsUrl = apiUrl.replace("http://", "ws://").replace("https://", "wss://");
  const url = `${wsUrl}/api/servers/${serverId}/ws?token=${encodeURIComponent(token)}`;

  const ws = new WebSocket(url);
  const messages: WsMessage[] = [];
  const logs: LogLine[] = [];

  // Wait for connection to open
  await new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(() => {
      reject(new Error("WebSocket connection timeout"));
    }, 10000);

    ws.onopen = () => {
      clearTimeout(timeout);
      resolve();
    };

    ws.onerror = (err) => {
      clearTimeout(timeout);
      reject(new Error(`WebSocket error: ${err}`));
    };
  });

  // Handle incoming messages
  ws.onmessage = (event) => {
    try {
      const msg = JSON.parse(event.data) as WsMessage;
      messages.push(msg);

      if (msg.type === "Log") {
        logs.push(msg.data as LogLine);
      }
    } catch (err) {
      console.error("Failed to parse WebSocket message:", err);
    }
  };

  const close = () => {
    if (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING) {
      ws.close();
    }
  };

  const waitForLog = async (
    predicate: (log: LogLine) => boolean,
    timeout = 5000
  ): Promise<LogLine> => {
    const startTime = Date.now();

    while (Date.now() - startTime < timeout) {
      const found = logs.find(predicate);
      if (found) {
        return found;
      }

      await new Promise((resolve) => setTimeout(resolve, 100));
    }

    throw new Error(`Log matching predicate not found within ${timeout}ms`);
  };

  const waitForStatus = async (status: string, timeout = 5000): Promise<void> => {
    const startTime = Date.now();

    while (Date.now() - startTime < timeout) {
      const statusMsg = messages.find(
        (m) => m.type === "StatusChange" && (m.data as any)?.status === status
      );
      if (statusMsg) {
        return;
      }

      await new Promise((resolve) => setTimeout(resolve, 100));
    }

    throw new Error(`Status '${status}' not received within ${timeout}ms`);
  };

  return {
    ws,
    messages,
    logs,
    close,
    waitForLog,
    waitForStatus,
  };
}

/**
 * Wait for a WebSocket connection to close.
 */
export async function waitForClose(ws: WebSocket, timeout = 5000): Promise<void> {
  if (ws.readyState === WebSocket.CLOSED) {
    return;
  }

  return new Promise<void>((resolve, reject) => {
    const timer = setTimeout(() => {
      reject(new Error("WebSocket did not close within timeout"));
    }, timeout);

    ws.onclose = () => {
      clearTimeout(timer);
      resolve();
    };
  });
}

/**
 * Send a message through a WebSocket and wait for a response.
 */
export async function sendAndWaitForResponse(
  ws: WebSocket,
  message: string,
  responseCheck: (msg: WsMessage) => boolean,
  timeout = 5000
): Promise<WsMessage> {
  const messages: WsMessage[] = [];

  const handler = (event: MessageEvent) => {
    try {
      const msg = JSON.parse(event.data) as WsMessage;
      messages.push(msg);
    } catch (err) {
      console.error("Failed to parse WebSocket message:", err);
    }
  };

  ws.addEventListener("message", handler);

  try {
    ws.send(message);

    const startTime = Date.now();
    while (Date.now() - startTime < timeout) {
      const found = messages.find(responseCheck);
      if (found) {
        return found;
      }

      await new Promise((resolve) => setTimeout(resolve, 100));
    }

    throw new Error(`Response not received within ${timeout}ms`);
  } finally {
    ws.removeEventListener("message", handler);
  }
}
