import { describe, it, expect } from "vitest";
import { bytesToB64 } from "@/delivery/b64";
import {
  blindSignToken,
  blindTokenRequest,
  finalizeToken,
  generateInboxKeyPair,
  verifyPrepared,
} from "@/delivery/rsabssa";
import { deriveContactTag } from "@/delivery/tokenMessage";

describe("RSABSSA delivery tokens", () => {
  it("blind issue → verify round-trip", async () => {
    const { publicKey, privateKey } = await generateInboxKeyPair(2048);
    const t = crypto.getRandomValues(new Uint8Array(32));
    const ct = await deriveContactTag("5Bob", "5Alice");
    const epoch = 0;
    const { blindedMsg, inv, preparedMsg } = await blindTokenRequest(publicKey, t, ct, epoch);
    const blindSig = await blindSignToken(privateKey, blindedMsg);
    const sig = await finalizeToken(publicKey, preparedMsg, blindSig, inv);
    expect(await verifyPrepared(publicKey, sig, preparedMsg)).toBe(true);
    expect(bytesToB64(sig).length).toBeGreaterThan(100);
  });
});
