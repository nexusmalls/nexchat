// EN: OPK Merkle tree (design §19). The chain stores only the root (`DeviceOpkRoots`);
// the relay distributes individual OPK leaves + a membership proof, and the X3DH
// initiator verifies the proof against the on-chain root (relay-trustless). This module
// is the FROZEN reference: leaves are the 32-byte OPK public keys sorted ascending,
// domain-separated hashing prevents leaf/node second-preimage confusion, odd nodes are
// promoted (carried up) unchanged.
// CN: OPK Merkle 树（设计 §19）。链上只存根（`DeviceOpkRoots`）；relay 分发单条 OPK 叶子 +
// 成员证明，X3DH 发起方用链上根验证（relay-trustless）。本模块为冻结参考实现：叶子为升序
// 排序的 32 字节 OPK 公钥，域分隔哈希防叶/节点二次原像混淆，奇数节点原样上提。

import { blake2AsU8a } from "@polkadot/util-crypto";

const LEAF_PREFIX = 0x00;
const NODE_PREFIX = 0x01;
const OPK_LEN = 32;

/// EN: One step of a Merkle membership proof: a sibling hash and which side it is on.
/// CN: Merkle 成员证明的一步：兄弟节点哈希及其所在侧。
export interface OpkProofStep {
  sibling: Uint8Array; // 32 bytes
  siblingIsLeft: boolean;
}

const leafHash = (opk: Uint8Array): Uint8Array => {
  const buf = new Uint8Array(1 + opk.length);
  buf[0] = LEAF_PREFIX;
  buf.set(opk, 1);
  return blake2AsU8a(buf, 256);
};

const nodeHash = (left: Uint8Array, right: Uint8Array): Uint8Array => {
  const buf = new Uint8Array(1 + left.length + right.length);
  buf[0] = NODE_PREFIX;
  buf.set(left, 1);
  buf.set(right, 1 + left.length);
  return blake2AsU8a(buf, 256);
};

const cmp = (a: Uint8Array, b: Uint8Array): number => {
  for (let i = 0; i < a.length && i < b.length; i++) {
    if (a[i] !== b[i]) return a[i] - b[i];
  }
  return a.length - b.length;
};

const eq = (a: Uint8Array, b: Uint8Array): boolean =>
  a.length === b.length && a.every((x, i) => x === b[i]);

/// EN: Sorted copy of the OPK public keys (ascending). The canonical leaf order that
/// both root computation and proofs use. CN: OPK 公钥的升序副本（规范叶序，根与证明共用）。
export function sortOpks(opks: Uint8Array[]): Uint8Array[] {
  for (const k of opks) {
    if (k.length !== OPK_LEN) throw new Error(`opkMerkle: OPK must be ${OPK_LEN} bytes`);
  }
  return [...opks].sort(cmp);
}

/// EN: Compute the Merkle root over an OPK set (design §19, frozen). Throws on empty set.
/// CN: 计算 OPK 集合的 Merkle 根（设计 §19，冻结）。空集抛错。
export function opkMerkleRoot(opks: Uint8Array[]): Uint8Array {
  if (opks.length === 0) throw new Error("opkMerkleRoot: empty OPK set");
  let level = sortOpks(opks).map(leafHash);
  while (level.length > 1) {
    const next: Uint8Array[] = [];
    for (let i = 0; i < level.length; i += 2) {
      next.push(i + 1 < level.length ? nodeHash(level[i], level[i + 1]) : level[i]);
    }
    level = next;
  }
  return level[0];
}

/// EN: Build a membership proof for `opk` within `opks`. CN: 为 `opks` 中的 `opk` 构造成员证明。
export function opkMerkleProof(opks: Uint8Array[], opk: Uint8Array): OpkProofStep[] {
  const sorted = sortOpks(opks);
  let idx = sorted.findIndex((k) => eq(k, opk));
  if (idx < 0) throw new Error("opkMerkleProof: opk not in set");
  const proof: OpkProofStep[] = [];
  let level = sorted.map(leafHash);
  while (level.length > 1) {
    const next: Uint8Array[] = [];
    for (let i = 0; i < level.length; i += 2) {
      if (i + 1 < level.length) {
        next.push(nodeHash(level[i], level[i + 1]));
        if (i === idx) proof.push({ sibling: level[i + 1], siblingIsLeft: false });
        else if (i + 1 === idx) proof.push({ sibling: level[i], siblingIsLeft: true });
      } else {
        next.push(level[i]); // promoted, no sibling
      }
    }
    idx = Math.floor(idx / 2);
    level = next;
  }
  return proof;
}

/// EN: Verify that `opk` is in the set committed by `root` (relay-trustless check the
/// X3DH initiator runs against the on-chain root). CN: 验证 `opk` 属于 `root` 承诺的集合
/// （X3DH 发起方对链上根做的 relay-trustless 校验）。
export function verifyOpkProof(root: Uint8Array, opk: Uint8Array, proof: OpkProofStep[]): boolean {
  if (opk.length !== OPK_LEN) return false;
  let node = leafHash(opk);
  for (const step of proof) {
    node = step.siblingIsLeft ? nodeHash(step.sibling, node) : nodeHash(node, step.sibling);
  }
  return eq(node, root);
}

const STEP_LEN = 33; // 32-byte sibling + 1-byte side flag

/// EN: Encode a Merkle proof to bytes for relay control-plane transport (§21
/// `opk_publish.leaves[].proof`): each step is `sibling[32] ‖ side(1)` (`1` = sibling is
/// left). CN: 把 Merkle 证明编码为字节，供 relay 控制面传输（§21 `opk_publish` 的 proof）：
/// 每步为 `sibling[32] ‖ side(1)`（`1` = 兄弟在左）。
export function encodeOpkProof(proof: OpkProofStep[]): Uint8Array {
  const out = new Uint8Array(proof.length * STEP_LEN);
  proof.forEach((step, i) => {
    if (step.sibling.length !== 32) throw new Error("encodeOpkProof: sibling must be 32 bytes");
    out.set(step.sibling, i * STEP_LEN);
    out[i * STEP_LEN + 32] = step.siblingIsLeft ? 1 : 0;
  });
  return out;
}

/// EN: Decode a Merkle proof from `encodeOpkProof` bytes. Throws on length mismatch.
/// CN: 由 `encodeOpkProof` 字节解码 Merkle 证明。长度不符时抛错。
export function decodeOpkProof(bytes: Uint8Array): OpkProofStep[] {
  if (bytes.length % STEP_LEN !== 0) {
    throw new Error(`decodeOpkProof: length ${bytes.length} not a multiple of ${STEP_LEN}`);
  }
  const proof: OpkProofStep[] = [];
  for (let i = 0; i < bytes.length; i += STEP_LEN) {
    proof.push({ sibling: bytes.slice(i, i + 32), siblingIsLeft: bytes[i + 32] === 1 });
  }
  return proof;
}
