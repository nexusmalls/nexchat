import { describe, expect, it, vi } from "vitest";

describe("networkRelayRequired", () => {
  it("is false in mock mode even when relay WS is configured", async () => {
    vi.stubEnv("VITE_USE_MOCK", "true");
    vi.stubEnv("VITE_RELAY_WS", "ws://127.0.0.1:8765");
    vi.resetModules();
    const { networkRelayRequired } = await import("@/relay/relayNetwork");
    expect(networkRelayRequired()).toBe(false);
    vi.unstubAllEnvs();
  });

  it("is true when relay WS is set and mock is off", async () => {
    vi.stubEnv("VITE_USE_MOCK", "false");
    vi.stubEnv("VITE_RELAY_WS", "wss://relay.example/ws");
    vi.resetModules();
    const { networkRelayRequired } = await import("@/relay/relayNetwork");
    expect(networkRelayRequired()).toBe(true);
    vi.unstubAllEnvs();
  });
});
