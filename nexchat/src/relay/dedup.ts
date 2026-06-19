// EN: Inbound frame dedup for multiplex relay (BC + WebSocket may deliver duplicates).
// CN: 多路 relay 入站去重（BC + WebSocket 可能重复投递）。

const MAX = 2000;

export class InboundDedup {
  private seen = new Set<string>();
  private order: string[] = [];

  /// EN: Returns true if this key is new (should deliver). CN: 新键返回 true（应投递）。
  accept(key: string): boolean {
    if (this.seen.has(key)) return false;
    this.seen.add(key);
    this.order.push(key);
    if (this.order.length > MAX) {
      const drop = this.order.splice(0, MAX / 2);
      for (const k of drop) this.seen.delete(k);
    }
    return true;
  }
}

export function frameDedupKey(frame: {
  dedupKey?: string;
  convId: string;
  ciphertextB64: string;
}): string {
  return frame.dedupKey ?? `${frame.convId}:${frame.ciphertextB64}`;
}
