// EN: Resolve group member SS58 list for relay targeted frame delivery.
// CN: 解析群成员 SS58 列表，供 relay 定向投递聊天帧。

import { chainClient } from "@/chain/chainClient";

const cache = new Map<number, { at: number; addrs: string[] }>();
const TTL_MS = 60_000;

/// EN: Member addresses for `g:{groupId}` conv routing hint. CN: `g:{groupId}` 的路由成员地址。
export async function groupRouteTo(convId: string): Promise<string[] | undefined> {
  if (!convId.startsWith("g:")) return undefined;
  const gid = Number(convId.slice(2));
  if (!Number.isFinite(gid)) return undefined;
  const hit = cache.get(gid);
  if (hit && Date.now() - hit.at < TTL_MS) return hit.addrs;
  try {
    const members = await chainClient.listGroupMembers(gid);
    const addrs = members.map((m) => m.address);
    cache.set(gid, { at: Date.now(), addrs });
    return addrs;
  } catch {
    return hit?.addrs;
  }
}
