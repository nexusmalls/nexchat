import { describe, expect, it } from "vitest";
import { DmKind, type DmEnvelope } from "@/crypto-dr/types";
import {
  DM_ENVELOPE_VER,
  decodeDmEnvelope,
  deviceIdFromIk,
  encodeDmEnvelope,
  peekDmHeader,
} from "@/crypto-dr/dmEnvelope";

function mkEnv(overrides: Partial<DmEnvelope> = {}): DmEnvelope {
  return {
    ver: DM_ENVELOPE_VER,
    kind: DmKind.Msg,
    senderDev: new Uint8Array(16).fill(0xa1),
    recvDev: new Uint8Array(16).fill(0xb2),
    prekeyEpoch: 7n,
    body: new Uint8Array([1, 2, 3, 4, 5]),
    ...overrides,
  };
}

describe("DmEnvelope codec", () => {
  it("round-trips a Msg envelope", () => {
    const env = mkEnv();
    const decoded = decodeDmEnvelope(encodeDmEnvelope(env));
    expect(decoded.ver).toBe(env.ver);
    expect(decoded.kind).toBe(DmKind.Msg);
    expect(decoded.senderDev).toEqual(env.senderDev);
    expect(decoded.recvDev).toEqual(env.recvDev);
    expect(decoded.prekeyEpoch).toBe(7n);
    expect(decoded.body).toEqual(env.body);
  });

  it("round-trips an Init envelope with empty body", () => {
    const env = mkEnv({ kind: DmKind.Init, body: new Uint8Array(0) });
    const decoded = decodeDmEnvelope(encodeDmEnvelope(env));
    expect(decoded.kind).toBe(DmKind.Init);
    expect(decoded.body.length).toBe(0);
  });

  it("preserves a full u64 prekeyEpoch", () => {
    const big = 0xffff_ffff_ffff_ffffn;
    const decoded = decodeDmEnvelope(encodeDmEnvelope(mkEnv({ prekeyEpoch: big })));
    expect(decoded.prekeyEpoch).toBe(big);
  });

  it("header is the first 42 cleartext bytes (relay-routable)", () => {
    const env = mkEnv();
    const bytes = encodeDmEnvelope(env);
    const hdr = peekDmHeader(bytes);
    expect(hdr.ver).toBe(DM_ENVELOPE_VER);
    expect(hdr.kind).toBe(DmKind.Msg);
    expect(hdr.senderDev).toEqual(env.senderDev);
    expect(hdr.recvDev).toEqual(env.recvDev);
    expect(hdr.prekeyEpoch).toBe(7n);
  });

  it("rejects truncated input", () => {
    expect(() => decodeDmEnvelope(new Uint8Array(10))).toThrow();
  });

  it("rejects a body-length mismatch", () => {
    const bytes = encodeDmEnvelope(mkEnv());
    expect(() => decodeDmEnvelope(bytes.slice(0, bytes.length - 1))).toThrow();
  });

  it("rejects an unknown kind", () => {
    const bytes = encodeDmEnvelope(mkEnv());
    bytes[1] = 0x09;
    expect(() => decodeDmEnvelope(bytes)).toThrow();
  });

  it("rejects wrong-length device ids on encode", () => {
    expect(() => encodeDmEnvelope(mkEnv({ senderDev: new Uint8Array(8) }))).toThrow();
  });
});

describe("deviceIdFromIk (cross-language frozen vector)", () => {
  it("matches the pallet-msg-identity golden for ik = [0x11; 32]", () => {
    // Must equal sp_io::hashing::blake2_128([0x11; 32]) asserted in the Rust pallet
    // (pallets/chat/msg-identity/src/tests.rs::frozen_device_id_derivation).
    const ik = new Uint8Array(32).fill(0x11);
    const dev = deviceIdFromIk(ik);
    const golden = new Uint8Array([
      0x7f, 0x9c, 0x29, 0x9f, 0x1d, 0x9b, 0xbe, 0x85, 0x6f, 0xbf, 0x2c, 0x98, 0xf0, 0xf9, 0x14,
      0x35,
    ]);
    expect(dev).toEqual(golden);
  });

  it("rejects a wrong-length IK", () => {
    expect(() => deviceIdFromIk(new Uint8Array(16))).toThrow();
  });
});
