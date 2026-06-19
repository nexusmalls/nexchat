// EN: Cloud pointer put stale recovery. CN: 云指针 put 的 stale 恢复。

import { beforeEach, describe, expect, it, vi } from "vitest";
import { RelayStalePointerError } from "@/relay/relayErrors";

const relayOneShotSend = vi.hoisted(() => vi.fn());
const fetchRemote = vi.hoisted(() => vi.fn());

vi.mock("@/config", () => ({ config: { relayWs: "ws://test" } }));
vi.mock("@/relay/relayOneShot", () => ({ relayOneShotSend }));

import { publishCloudPointer } from "@/relay/pointerPut";

describe("publishCloudPointer", () => {
  const writeLocal = vi.fn();

  beforeEach(() => {
    relayOneShotSend.mockReset();
    fetchRemote.mockReset();
    writeLocal.mockReset();
  });

  it("refetches and writes local when remote LWW wins", async () => {
    relayOneShotSend.mockRejectedValueOnce(new RelayStalePointerError("index_reject", 99));
    fetchRemote.mockResolvedValueOnce({ cid: "bafyRemote", updated_at: 99 });
    await publishCloudPointer(
      "5Alice",
      "index_put",
      "index_ack",
      { cid: "bafyLocal", updated_at: 50 },
      writeLocal,
      fetchRemote,
    );
    expect(fetchRemote).toHaveBeenCalledWith("5Alice");
    expect(writeLocal).toHaveBeenCalledWith("5Alice", { cid: "bafyRemote", updated_at: 99 });
  });

  it("rethrows non-stale errors", async () => {
    relayOneShotSend.mockRejectedValueOnce(new Error("timeout"));
    await expect(
      publishCloudPointer(
        "5Alice",
        "index_put",
        "index_ack",
        { cid: "bafy", updated_at: 1 },
        writeLocal,
        fetchRemote,
      ),
    ).rejects.toThrow("timeout");
  });
});
