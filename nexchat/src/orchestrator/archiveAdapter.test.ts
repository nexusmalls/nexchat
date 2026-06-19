import { describe, expect, it, vi } from "vitest";
import { type ArchivePusher, MsgArchivePort } from "@/orchestrator/archiveAdapter";

describe("MsgArchivePort — ArchivePort over MsgArchiveSync.push", () => {
  it("archive() pushes the current account snapshot", async () => {
    const push = vi.fn().mockResolvedValue(undefined);
    const pusher: ArchivePusher = { push };
    const port = new MsgArchivePort("5alice", { pusher });

    await port.archive("d:5bob");
    expect(push).toHaveBeenCalledWith("5alice");
  });

  it("propagates push failure so the orchestrator can surface a retry", async () => {
    const pusher: ArchivePusher = { push: vi.fn().mockRejectedValue(new Error("ipfs down")) };
    const port = new MsgArchivePort("5alice", { pusher });
    await expect(port.archive("d:5bob")).rejects.toThrow(/ipfs down/);
  });
});
