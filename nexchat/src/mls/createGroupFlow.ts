// EN: User-initiated on-chain group creation (create_group + first commit ≥2 members).
// CN: 用户发起的链上建群（create_group + 首个 commit 至少加 2 人）。

import type { ChainClient } from "@/chain/chainClient";
import { hex } from "@/mls/chainBytes";
import type { OpenMlsEngine } from "@/mls/openMlsEngine";
import { canonicalAddress } from "@/wallet/address";

const FUND_PLANCK = 100_000_000_000_000n;
const FUND_THRESHOLD = 10_000_000_000_000n;
const KP_WAIT_MS = 45_000;
const KP_POLL_MS = 2_000;

function sleep(ms: number): Promise<void> {
  return new Promise((r) => setTimeout(r, ms));
}

/// EN: Optionally fund members on dev chain (creator-only genesis). CN: dev 链上给成员充值。
async function fundMembers(chain: ChainClient, self: string, members: string[]): Promise<void> {
  for (const m of members) {
    if (m === self) continue;
    const bal = await chain.freeBalance(m);
    if (bal < FUND_THRESHOLD) {
      await chain.signAndSend("balances", "transferKeepAlive", [m, FUND_PLANCK]);
    }
  }
}

/// EN: Poll until ≥2 members have a published KeyPackage. CN: 轮询直到至少 2 人有 KeyPackage。
async function waitForKeyPackages(
  chain: ChainClient,
  members: string[],
  min = 2,
): Promise<{ addr: string; kp: Uint8Array }[]> {
  const deadline = Date.now() + KP_WAIT_MS;
  while (Date.now() < deadline) {
    const ready: { addr: string; kp: Uint8Array }[] = [];
    for (const addr of members) {
      const kps = await chain.keyPackagesOf(addr);
      if (kps.length > 0) ready.push({ addr, kp: kps[0]! });
    }
    if (ready.length >= min) return ready;
    await sleep(KP_POLL_MS);
  }
  throw new Error(
    "等待成员 KeyPackage 超时：请让对方先解锁 NexChat 并保持在线（需发布 KeyPackage）",
  );
}

export interface CreateGroupFlowDeps {
  engine: OpenMlsEngine;
  chain: ChainClient;
  selfAddress: string;
  name: string;
  memberAddresses: string[];
  isPublic?: boolean;
}

/// EN: Create chain group + MLS epoch-0 commit adding all ready members (≥2). Returns group id.
/// CN: 链上建群并用首个 commit 加入所有已就绪成员（≥2），返回群 id。
export async function createGroupWithMembers(deps: CreateGroupFlowDeps): Promise<number> {
  const self = canonicalAddress(deps.selfAddress);
  const name = deps.name.trim();
  if (!name) throw new Error("请输入群名称");

  const members = [
    ...new Set(
      deps.memberAddresses.map(canonicalAddress).filter((a) => a && a !== self),
    ),
  ];
  if (members.length < 2) {
    throw new Error("请至少选择 2 位联系人（链上群首个 commit 须一次加入 ≥2 人）");
  }

  await fundMembers(deps.chain, self, members);

  const predicted = await deps.chain.nextGroupId();
  const fp = deps.engine.createGroup(predicted);
  const gid = await deps.chain.createGroup([
    hex(new Uint8Array([1])),
    deps.engine.cipherSuite(),
    deps.isPublic ?? true,
    hex(fp.tree_hash),
    hex(fp.transcript_hash),
  ]);
  if (gid !== predicted) {
    throw new Error(`群 id 冲突：预期 ${predicted}，实际 ${gid}`);
  }

  await deps.chain.setGroupProfile(gid, { name });

  const ready = await waitForKeyPackages(deps.chain, members, 2);
  const epoch = deps.engine.epochOf(gid);
  const kps = ready.map((r) => r.kp);
  const out = deps.engine.addMembers(gid, kps);
  const welcomes = ready.map((r) => [r.addr, hex(out.welcome)] as [string, string]);
  await deps.chain.signAndSend("chatGroup", "commit", [
    gid,
    epoch,
    hex(out.commit),
    hex(out.tree_hash),
    hex(out.transcript_hash),
    hex(new Uint8Array([2])),
    welcomes,
    { added: ready.map((r) => r.addr), removed: [] },
  ]);

  return gid;
}
