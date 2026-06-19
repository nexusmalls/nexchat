import { describe, expect, it, vi } from "vitest";

vi.mock("@/config", () => ({
  config: { relayStrictAuth: false },
}));

import {
  buildRegisterAccountSignPayload,
  normalizeRelayAccount,
  registerAccountWire,
} from "@/relay/registerAccountAuth";
import { setSignerPair } from "@/chain/signer";
import { Keyring } from "@polkadot/keyring";

describe("registerAccountAuth", () => {
  it("builds canonical v1 payload bytes", () => {
    const account = normalizeRelayAccount("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY");
    const payload = buildRegisterAccountSignPayload("dev1", account);
    const text = new TextDecoder().decode(payload);
    expect(text.startsWith("nexchat-relay-register-v1\0")).toBe(true);
    expect(text).toContain("dev1");
    expect(text.endsWith(account)).toBe(true);
  });

  it("omits account_sig when relayStrictAuth is off", () => {
    const kr = new Keyring({ type: "sr25519", ss58Format: 42 });
    setSignerPair(kr.addFromUri("//Alice"));
    const wire = registerAccountWire("dev1", kr.addFromUri("//Bob").address);
    expect(wire.account_sig).toBeUndefined();
  });
});
