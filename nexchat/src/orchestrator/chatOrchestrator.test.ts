import { describe, expect, it } from "vitest";
import { ChatOrchestrator } from "@/orchestrator/chatOrchestrator";
import type { ArchivePort, DrSessionPort, GroupId, MlsGroupPort } from "@/orchestrator/ports";

/// Deferred helper to hold an async op open (for the lock test).
function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

class FakeDr implements DrSessionPort {
  active = new Set<string>();
  frozen = new Set<string>();
  failOpen = false;
  log: string[] = [];
  async open(peer: string) {
    this.log.push(`open:${peer}`);
    if (this.failOpen) throw new Error("open boom");
    this.active.add(peer);
    this.frozen.delete(peer);
  }
  async freeze(peer: string) {
    this.log.push(`freeze:${peer}`);
    this.frozen.add(peer);
  }
  async resume(peer: string) {
    this.log.push(`resume:${peer}`);
    this.frozen.delete(peer);
  }
  async retire(peer: string) {
    this.log.push(`retire:${peer}`);
    this.active.delete(peer);
    this.frozen.delete(peer);
  }
  isActive(peer: string) {
    return this.active.has(peer) && !this.frozen.has(peer);
  }
}

class FakeMls implements MlsGroupPort {
  groups = new Map<GroupId, boolean>();
  nextId = 1;
  failCreate = false;
  failDissolve = false;
  onCreate?: () => Promise<void>;
  async createGroup(_members: string[]): Promise<GroupId> {
    if (this.onCreate) await this.onCreate();
    if (this.failCreate) throw new Error("create boom");
    const id = this.nextId++;
    this.groups.set(id, true);
    return id;
  }
  async dissolve(groupId: GroupId) {
    if (this.failDissolve) throw new Error("dissolve boom");
    this.groups.set(groupId, false);
  }
  isActive(groupId: GroupId) {
    return this.groups.get(groupId) === true;
  }
}

class FakeArchive implements ArchivePort {
  archived: string[] = [];
  async archive(convId: string) {
    this.archived.push(convId);
  }
}

const CONV = "d:5peer";
const PEER = "5peer";

function setup() {
  const dr = new FakeDr();
  const mls = new FakeMls();
  const archive = new FakeArchive();
  const orch = new ChatOrchestrator(dr, mls, archive);
  return { dr, mls, archive, orch };
}

/// Hard invariant: a conversation is never DR-active AND group-active at the same time.
function assertNeverBothActive(dr: FakeDr, mls: FakeMls, groupId?: GroupId) {
  const drOn = dr.isActive(PEER);
  const mlsOn = groupId != null && mls.isActive(groupId);
  expect(drOn && mlsOn).toBe(false);
}

describe("ChatOrchestrator — 2↔3 transition (design §11)", () => {
  it("2→3 happy: freezes DR, creates group, retires DR; history archived", async () => {
    const { dr, mls, archive, orch } = setup();
    orch.trackDirect(CONV, PEER);
    await dr.open(PEER); // pre-switch: DR active

    const res = await orch.promoteToGroup(CONV, PEER, ["5carol"]);
    expect(res).toMatchObject({ ok: true, mode: "group", version: 1 });
    const groupId = res.ok && res.mode === "group" ? res.groupId : undefined;

    expect(archive.archived).toContain(CONV);
    expect(dr.isActive(PEER)).toBe(false); // retired
    expect(mls.isActive(groupId!)).toBe(true);
    expect(orch.getMode(CONV)).toBe("group");
    assertNeverBothActive(dr, mls, groupId);
    // order: freeze before create, retire after
    expect(dr.log).toEqual([`open:${PEER}`, `freeze:${PEER}`, `retire:${PEER}`]);
  });

  it("2→3 rollback: createGroup fails → DR resumed, group not created", async () => {
    const { dr, mls, archive, orch } = setup();
    orch.trackDirect(CONV, PEER);
    await dr.open(PEER);
    mls.failCreate = true;

    const res = await orch.promoteToGroup(CONV, PEER, ["5carol"]);
    expect(res).toMatchObject({ ok: false, rolledBack: true });
    expect(dr.isActive(PEER)).toBe(true); // resumed → still active
    expect(mls.groups.size).toBe(0);
    expect(orch.getMode(CONV)).toBe("direct"); // unchanged
    expect(archive.archived).toContain(CONV);
    assertNeverBothActive(dr, mls);
  });

  it("3→2 happy: dissolves group, re-opens DR", async () => {
    const { dr, mls, archive, orch } = setup();
    const gid = await mls.createGroup([PEER, "5carol"]);
    orch["states"].set(CONV, { convId: CONV, mode: "group", version: 1, peer: PEER, groupId: gid, ready: true });

    const res = await orch.demoteToDirect(CONV, gid, PEER);
    expect(res).toMatchObject({ ok: true, mode: "direct", version: 2 });
    expect(mls.isActive(gid)).toBe(false);
    expect(dr.isActive(PEER)).toBe(true);
    expect(orch.getMode(CONV)).toBe("direct");
    expect(archive.archived).toContain(CONV);
    assertNeverBothActive(dr, mls, gid);
  });

  it("3→2 rollback: dissolve fails → group stays active, DR not opened", async () => {
    const { dr, mls, orch } = setup();
    const gid = await mls.createGroup([PEER, "5carol"]);
    orch["states"].set(CONV, { convId: CONV, mode: "group", version: 1, peer: PEER, groupId: gid, ready: true });
    mls.failDissolve = true;

    const res = await orch.demoteToDirect(CONV, gid, PEER);
    expect(res).toMatchObject({ ok: false, rolledBack: true });
    expect(mls.isActive(gid)).toBe(true); // group survives
    expect(dr.isActive(PEER)).toBe(false); // not opened
    expect(orch.getMode(CONV)).toBe("group");
    assertNeverBothActive(dr, mls, gid);
  });

  it("3→2 open failure: committed to direct but pending; ensureDirect retries", async () => {
    const { dr, mls, orch } = setup();
    const gid = await mls.createGroup([PEER, "5carol"]);
    orch["states"].set(CONV, { convId: CONV, mode: "group", version: 1, peer: PEER, groupId: gid, ready: true });
    dr.failOpen = true;

    const res = await orch.demoteToDirect(CONV, gid, PEER);
    expect(res).toMatchObject({ ok: false, rolledBack: false }); // committed to direct, retry needed
    expect(mls.isActive(gid)).toBe(false); // group already dissolved
    expect(orch.getMode(CONV)).toBe("direct");
    expect(orch.getState(CONV)?.ready).toBe(false);
    assertNeverBothActive(dr, mls, gid);

    dr.failOpen = false;
    const retry = await orch.ensureDirect(CONV);
    expect(retry).toMatchObject({ ok: true, mode: "direct" });
    expect(dr.isActive(PEER)).toBe(true);
    expect(orch.getState(CONV)?.ready).toBe(true);
  });

  it("rejects a concurrent switch on the same conversation (single write-lock)", async () => {
    const { dr, mls, orch } = setup();
    orch.trackDirect(CONV, PEER);
    await dr.open(PEER);

    const gate = deferred<void>();
    mls.onCreate = () => gate.promise; // hold createGroup open

    const first = orch.promoteToGroup(CONV, PEER, ["5carol"]);
    // second attempt while the first holds the lock
    const second = await orch.promoteToGroup(CONV, PEER, ["5dave"]);
    expect(second).toMatchObject({ ok: false, rolledBack: true });
    expect(second.ok === false && second.error).toMatch(/in progress/);

    gate.resolve();
    expect(await first).toMatchObject({ ok: true, mode: "group" });
  });

  it("refuses to promote a conversation that is not in direct mode", async () => {
    const { mls, orch } = setup();
    const gid = await mls.createGroup([PEER, "5carol"]);
    orch["states"].set(CONV, { convId: CONV, mode: "group", version: 1, peer: PEER, groupId: gid, ready: true });
    const res = await orch.promoteToGroup(CONV, PEER, ["5dave"]);
    expect(res).toMatchObject({ ok: false, rolledBack: true });
  });
});
