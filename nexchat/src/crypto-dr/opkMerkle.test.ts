import { describe, expect, it } from "vitest";
import {
  opkMerkleProof,
  opkMerkleRoot,
  sortOpks,
  verifyOpkProof,
} from "@/crypto-dr/opkMerkle";

/// Deterministic distinct 32-byte OPK public keys.
const opk = (seed: number): Uint8Array => new Uint8Array(32).fill(seed);

describe("OPK Merkle (design §19)", () => {
  it("is deterministic and order-independent (leaves are sorted)", () => {
    const a = [opk(3), opk(1), opk(2), opk(5)];
    const b = [opk(5), opk(2), opk(1), opk(3)];
    expect(opkMerkleRoot(a)).toEqual(opkMerkleRoot(b));
  });

  it("single-leaf root equals that leaf's hash and verifies with an empty proof", () => {
    const set = [opk(7)];
    const root = opkMerkleRoot(set);
    expect(verifyOpkProof(root, opk(7), opkMerkleProof(set, opk(7)))).toBe(true);
  });

  it("every leaf verifies against the root (even count)", () => {
    const set = [opk(1), opk(2), opk(3), opk(4)];
    const root = opkMerkleRoot(set);
    for (const k of sortOpks(set)) {
      expect(verifyOpkProof(root, k, opkMerkleProof(set, k))).toBe(true);
    }
  });

  it("every leaf verifies against the root (odd count → promoted node)", () => {
    const set = [opk(1), opk(2), opk(3), opk(4), opk(5)];
    const root = opkMerkleRoot(set);
    for (const k of set) {
      expect(verifyOpkProof(root, k, opkMerkleProof(set, k))).toBe(true);
    }
  });

  it("rejects a proof for a key not in the set", () => {
    const set = [opk(1), opk(2), opk(3)];
    const root = opkMerkleRoot(set);
    const proof = opkMerkleProof(set, opk(2));
    expect(verifyOpkProof(root, opk(9), proof)).toBe(false);
  });

  it("rejects a tampered proof step", () => {
    const set = [opk(1), opk(2), opk(3), opk(4)];
    const root = opkMerkleRoot(set);
    const proof = opkMerkleProof(set, opk(1));
    proof[0].sibling = new Uint8Array(32).fill(0xff);
    expect(verifyOpkProof(root, opk(1), proof)).toBe(false);
  });

  it("rejects empty sets and wrong-length keys", () => {
    expect(() => opkMerkleRoot([])).toThrow();
    expect(() => opkMerkleRoot([new Uint8Array(16)])).toThrow();
  });
});
