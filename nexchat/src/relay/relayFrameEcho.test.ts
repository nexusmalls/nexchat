import { describe, it, expect, vi, afterEach } from "vitest";
import { withMultiDeviceEcho, type RelayFrame } from "@/relay/relayClient";
import { config } from "@/config";

const base: RelayFrame = {
  convId: "d:5Bob",
  senderRef: "5Alice",
  ciphertextB64: "abc",
};

describe("withMultiDeviceEcho", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("sets echoSelf for 1:1 when Wire multi-leaf is enabled", () => {
    vi.spyOn(config, "wireMultileafEnabled", "get").mockReturnValue(true);
    vi.spyOn(config, "wireGroupMultileafEnabled", "get").mockReturnValue(false);
    expect(withMultiDeviceEcho(base, base.convId).echoSelf).toBe(true);
  });

  it("sets echoSelf for groups when group Wire multi-leaf is enabled", () => {
    vi.spyOn(config, "wireMultileafEnabled", "get").mockReturnValue(false);
    vi.spyOn(config, "wireGroupMultileafEnabled", "get").mockReturnValue(true);
    expect(withMultiDeviceEcho({ ...base, convId: "g:1" }, "g:1").echoSelf).toBe(true);
  });

  it("leaves frame unchanged when Wire flags are off", () => {
    vi.spyOn(config, "wireMultileafEnabled", "get").mockReturnValue(false);
    vi.spyOn(config, "wireGroupMultileafEnabled", "get").mockReturnValue(false);
    expect(withMultiDeviceEcho(base, base.convId)).toEqual(base);
  });
});
