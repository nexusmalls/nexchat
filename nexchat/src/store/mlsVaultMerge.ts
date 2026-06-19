// EN: Track A MLS escrow-vault envelope + concurrent-write merge logic (design
// CHAT_MULTIDEVICE_MLS_SYNC §4.2/§4.4). The vault is a SINGLE encrypted blob, but the manifest
// carries only one `mls.cid`, so two devices refreshing different groups must NOT blind-overwrite
// (silent lost update). §4.4 exploits the MLS property that, given the HandshakeLog, epoch N of a
// group is deterministic across devices → merge degrades to a per-group max-epoch rule with a
// prev_cid CAS guard (optimistic concurrency); no CRDT needed.
//
// This module is PURE (no IO): it defines the on-disk envelope codec and the merge DECISION a
// device makes before publishing. The monolithic OpenMLS blob cannot be field-merged in place, so
// the safe interim rule is: publish only when our aggregate per-group state dominates the remote;
// when the remote is strictly newer (or genuinely divergent) we skip and let normal commit
// processing converge, then re-publish. True per-group `state_b64` splitting (§4.2) is a deeper
// follow-up tracked separately.
// CN: 路线 A MLS 托管 vault 信封 + 并发写合并逻辑（设计 §4.2/§4.4）。vault 是**单一加密 blob**，而 manifest
// 只有一个 `mls.cid`，故两设备刷新不同群时**禁止盲覆盖**（静默丢更新）。§4.4 利用「给定 HandshakeLog，群 epoch
// N 在各设备确定性一致」的性质 → 合并退化为逐群 max-epoch + prev_cid CAS（乐观并发），无需 CRDT。
//
// 本模块为**纯函数**（无 IO）：定义磁盘信封编解码与设备发布前的合并**决策**。单体 OpenMLS blob 无法原地按字段
// 合并，故安全的过渡规则为：仅当本机逐群聚合状态支配远端时才发布；远端严格更新（或真分叉）时跳过，靠正常
// commit 处理收敛后再发布。真正的逐群 `state_b64` 切分（§4.2）为后续更深的工作，单独跟踪。

/// EN: On-disk (pre-seal) MLS vault envelope. Sealed as `iv(12) || AES-256-GCM(JSON bytes)` under
/// K_mls_escrow, then uploaded to IPFS; only the CID + updated_at land in the SyncManifest. The raw
/// OpenMLS escrow blob (signature key stripped, §3.2) sits in `blob` (base64). `groups` maps the
/// frontend conversation key → snapshot epoch, used for the per-group merge. `prevCid` records the
/// remote CID this write was rebased on (CAS). `deviceSeq` is a per-device monotone counter used
/// only as an equal-epoch tiebreak. CN: 磁盘（封装前）MLS vault 信封。用 K_mls_escrow 封装为
/// `iv(12) || AES-256-GCM(JSON 字节)` 后上传 IPFS；仅 CID + updated_at 进 SyncManifest。剔除签名钥的
/// 原始 OpenMLS 托管 blob（§3.2）置于 `blob`（base64）。`groups` 为会话键→快照 epoch，用于逐群合并。
/// `prevCid` 记录本次写入 rebase 的远端 CID（CAS）。`deviceSeq` 为设备级单调计数，仅作同 epoch 平局裁决。
export interface VaultEnvelope {
  v: 1;
  updated_at: number;
  deviceSeq: number;
  groups: Record<string, number>;
  prevCid: string | null;
  blob: string;
}

const enc = new TextEncoder();
const dec = new TextDecoder();

export function encodeVaultEnvelope(env: VaultEnvelope): Uint8Array {
  return enc.encode(JSON.stringify(env));
}

/// EN: Decode + validate an envelope; throws on shape/version mismatch. CN: 解码并校验信封；形状/版本
/// 不符即抛错。
export function decodeVaultEnvelope(bytes: Uint8Array): VaultEnvelope {
  const obj = JSON.parse(dec.decode(bytes)) as Partial<VaultEnvelope>;
  if (obj.v !== 1) throw new Error(`unsupported mls-vault envelope version ${obj.v}`);
  if (typeof obj.blob !== "string" || !obj.blob) throw new Error("mls-vault envelope missing blob");
  if (typeof obj.groups !== "object" || obj.groups === null) {
    throw new Error("mls-vault envelope missing groups");
  }
  return {
    v: 1,
    updated_at: typeof obj.updated_at === "number" ? obj.updated_at : 0,
    deviceSeq: typeof obj.deviceSeq === "number" ? obj.deviceSeq : 0,
    groups: obj.groups as Record<string, number>,
    prevCid: typeof obj.prevCid === "string" ? obj.prevCid : null,
    blob: obj.blob,
  };
}

/// EN: A device's publish decision after reading the current remote pointer (§4.4). CN: 设备读取当前
/// 远端指针后的发布决策（§4.4）。
export type VaultMergeAction =
  /// EN: no concurrent writer since our base → publish as-is. CN: 自 base 以来无并发写 → 直接发布。
  | { action: "publish" }
  /// EN: a concurrent writer advanced the pointer, but our per-group state strictly dominates →
  /// re-CAS on the observed remote cid and publish. CN: 有并发写推进了指针，但本机逐群状态严格支配 →
  /// 以观察到的远端 cid 重新 CAS 并发布。
  | { action: "publish-rebased"; basedOnCid: string }
  /// EN: do not publish this round. `remote-newer`: we are behind (catch up via commits, then
  /// retry); `divergent`: each side is ahead on different groups (let commit processing converge);
  /// `no-op`: identical per-group epochs (nothing to gain). CN: 本轮不发布。`remote-newer`：本机落后
  /// （靠 commit 补齐后重试）；`divergent`：双方各自在不同群更新（靠 commit 处理收敛）；`no-op`：逐群
  /// epoch 相同（无新信息）。
  | { action: "skip"; reason: "remote-newer" | "divergent" | "no-op" };

/// EN: Decide whether/how to publish the local vault given the remote one. Pure. The prev_cid CAS:
/// when `remoteCid === prevCid` no one wrote since our base, so a plain publish is safe; otherwise we
/// compare per-group max epoch to avoid clobbering a concurrent device. CN: 在已知远端的情况下，决定
/// 是否/如何发布本机 vault。纯函数。prev_cid CAS：`remoteCid === prevCid` 时自 base 起无人写入，可安全
/// 发布；否则比较逐群 max epoch，避免覆盖并发设备。
export function decideVaultMerge(args: {
  localGroups: Record<string, number>;
  remote: VaultEnvelope | null;
  prevCid: string | null;
  remoteCid: string | null;
}): VaultMergeAction {
  const { localGroups, remote, prevCid, remoteCid } = args;

  // EN: No concurrent write since our base (cid unchanged), or the remote slot is empty → publish.
  // CN: 自 base 起无并发写（cid 未变），或远端槽为空 → 发布。
  if (remoteCid === prevCid || !remote || !remoteCid) {
    return { action: "publish" };
  }

  const ids = new Set<string>([...Object.keys(localGroups), ...Object.keys(remote.groups)]);
  let localAhead = false;
  let remoteAhead = false;
  for (const id of ids) {
    const l = localGroups[id] ?? 0;
    const r = remote.groups[id] ?? 0;
    if (l > r) localAhead = true;
    else if (r > l) remoteAhead = true;
  }

  if (remoteAhead && !localAhead) return { action: "skip", reason: "remote-newer" };
  if (localAhead && !remoteAhead) return { action: "publish-rebased", basedOnCid: remoteCid };
  if (localAhead && remoteAhead) return { action: "skip", reason: "divergent" };
  return { action: "skip", reason: "no-op" };
}
