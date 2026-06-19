// EN: Multi-device routing for the DR stack — "Scheme A" (design §8 / §18.3): account A
// (devices a1,a2) ↔ account B (device b1) maintains ONE independent Double Ratchet session
// per peer DEVICE (a1↔b1, a2↔b1, …), never a per-account session. To send to an account,
// the initiator encrypts a SEPARATE copy for each of the peer's registered devices over that
// device's own session and emits one `DmEnvelope` per device (`recv_dev` routes it; the
// receiver processes only frames addressed to it, already enforced in `DrTransport`).
//
// NONCE RED LINE (design §8 / §18.3): each Olm session has a SINGLE owner and its sending
// chain is NEVER shared across devices — fan-out is N independent encryptions, not one
// ciphertext reused, so two devices can never derive the same (key, nonce). This module only
// loops/﻿discovers/establishes sessions; the crypto stays one-session-per-device inside the
// engine. Import-decoupled from `@/mls/*`.
// CN: DR 栈的多设备路由——「方案 A」（设计 §8 / §18.3）：账户 A（设备 a1,a2）↔ 账户 B（设备 b1）
// 对**每个对端设备**各维护一条独立双棘轮会话（a1↔b1、a2↔b1…），而非每账户一条。给账户发消息时，
// 发起方对对端每个已注册设备在其各自会话上**各加密一份**，每设备发一条 `DmEnvelope`（`recv_dev`
// 路由；接收方只处理寻址给自己的帧，已由 `DrTransport` 强制）。
//
// nonce 红线（设计 §8 / §18.3）：每条 Olm 会话单一持有者、发送链绝不跨设备共享——扇出是 N 次独立
// 加密而非复用一份密文，故两设备永不会派生出相同 (key, nonce)。本模块只做循环/发现/建会话，密码学
// 保持引擎内每设备一会话。与 `@/mls/*` import 解耦。

import { chainClient } from "@/chain/chainClient";
import type { DrTransport } from "@/crypto-dr/drTransport";
import { fetchPeerBundleWithOpk } from "@/crypto-dr/opkExchange";
import type { DeviceId, PeerPrekeyBundle } from "@/crypto-dr/types";
import type { RelayClient } from "@/relay/relayClient";
import { canonicalAddress } from "@/wallet/address";

const eqBytes = (a: Uint8Array, b: Uint8Array): boolean =>
  a.length === b.length && a.every((x, i) => x === b[i]);

/// EN: Discovers an account's registered device ids. CN: 发现账户已注册设备 id。
export interface DeviceDirectory {
  listDevices(account: string): Promise<DeviceId[]>;
}

/// EN: Chain-backed device directory with a short-TTL cache and a control-plane REFRESH
/// SUBSCRIPTION. Fan-out reads the roster on every send, so an uncached query would hit the chain
/// per message; the cache collapses bursts while a peer device joining (its `opk_publish` on the
/// shared relay control plane) invalidates that account so the next send re-reads and includes the
/// new device. Import-decoupled from `@/mls/*`. CN: 链上设备目录，带短 TTL 缓存与控制面**刷新订阅**。
/// 扇出每次发送都要读名册，无缓存会每条消息打链；缓存收敛突发，而对端新设备加入（其在共享 relay 控制
/// 面上的 `opk_publish`）使该账户失效，下次发送即重读并纳入新设备。与 `@/mls/*` import 解耦。
export class ChainDeviceDirectory implements DeviceDirectory {
  private readonly cache = new Map<string, DeviceId[]>();
  private readonly fetchedAt = new Map<string, number>();
  private readonly ttlMs: number;
  private unsub: (() => void) | null = null;

  constructor(opts?: { ttlMs?: number }) {
    this.ttlMs = opts?.ttlMs ?? 30_000;
  }

  private key(account: string): string {
    try {
      return canonicalAddress(account);
    } catch {
      return account;
    }
  }

  async listDevices(account: string): Promise<DeviceId[]> {
    const key = this.key(account);
    const at = this.fetchedAt.get(key);
    if (at !== undefined && Date.now() - at < this.ttlMs) {
      return this.cache.get(key) ?? [];
    }
    return this.refresh(account);
  }

  /// EN: Force a chain re-read of `account`'s device roster, updating the cache. CN: 强制重读
  /// `account` 设备名册并刷新缓存。
  async refresh(account: string): Promise<DeviceId[]> {
    const key = this.key(account);
    const devices = (await chainClient.msgIdentityDevices(account)).map((d) => d.deviceId);
    this.cache.set(key, devices);
    this.fetchedAt.set(key, Date.now());
    return devices;
  }

  /// EN: Drop `account`'s cached roster so the next `listDevices` re-reads the chain. CN: 丢弃
  /// `account` 缓存名册，使下次 `listDevices` 重读链。
  invalidate(account: string): void {
    const key = this.key(account);
    this.cache.delete(key);
    this.fetchedAt.delete(key);
  }

  /// EN: Subscribe to the relay control plane: an `opk_publish` means a device of `from` advertised
  /// prekeys (it just came online / newly registered) → invalidate that account so fan-out re-reads
  /// its (possibly grown) roster. `onControl` is fan-out, so this coexists with other consumers.
  /// CN: 订阅 relay 控制面：`opk_publish` 表示 `from` 的某设备发布了预密钥（刚上线/新注册）→ 使该
  /// 账户失效，让扇出重读其（可能增长的）名册。`onControl` 为扇出，故与其他消费者共存。
  subscribeRefresh(relay: RelayClient): void {
    if (this.unsub) return; // idempotent
    const handler = (msg: { t: string; from?: string }): void => {
      if (msg.t === "opk_publish" && msg.from) this.invalidate(msg.from);
    };
    relay.onControl(handler as Parameters<RelayClient["onControl"]>[0]);
    this.unsub = () => {};
  }
}

/// EN: Supplies a verified prekey bundle for a specific peer device (used to establish a
/// missing session). Returns null when the device cannot be bundled (unregistered / no SPK).
/// CN: 为特定对端设备提供已校验预密钥包（用于建立缺失会话）。无法装配（未注册/无 SPK）时返回 null。
export interface PeerBundleProvider {
  get(account: string, device: DeviceId): Promise<PeerPrekeyBundle | null>;
}

/// EN: Default provider: chain prekeys + relay-served OPK upgrade (SPK fallback), per device.
/// CN: 默认提供器：链上预密钥 + relay 单发 OPK 升级（SPK 回退），按设备。
export class RelayBundleProvider implements PeerBundleProvider {
  constructor(
    private readonly relay: RelayClient,
    private readonly selfRef: string,
    private readonly timeoutMs?: number,
  ) {}
  async get(account: string, device: DeviceId): Promise<PeerPrekeyBundle | null> {
    try {
      return await fetchPeerBundleWithOpk(this.relay, this.selfRef, account, {
        deviceId: device,
        timeoutMs: this.timeoutMs,
      });
    } catch {
      return null;
    }
  }
}

/// EN: Result of an account-level fan-out send. CN: 账户级扇出发送结果。
export interface MultiDeviceSendResult {
  /// EN: Devices a copy was encrypted + sent to. CN: 已加密并发送副本的设备。
  sentTo: DeviceId[];
  /// EN: Devices skipped (no bundle available — peer offline / unregistered). CN: 跳过的设备
  /// （无可用 bundle——对端离线/未注册）。
  skipped: DeviceId[];
}

/// EN: Account-level fan-out policy over a single-device `DrTransport`. Establishes a missing
/// per-device session lazily, then encrypts + sends one copy per device. The SAME method
/// drives peer fan-out and sibling-device echo (a sibling is just another device of an
/// account); the local device is always excluded to avoid a self-send loop. CN: 在单设备
/// `DrTransport` 之上的账户级扇出策略。按需懒建缺失的每设备会话，再对每个设备各加密各发一份。
/// 同一方法驱动对端扇出与兄弟设备回显（兄弟设备只是账户的另一个设备）；始终排除本设备以免自发环。
export class MultiDeviceRouter {
  constructor(
    private readonly transport: DrTransport,
    private readonly directory: DeviceDirectory,
    private readonly provider: PeerBundleProvider,
  ) {}

  /// EN: Encrypt + send `plaintext` to every registered device of `account` over its own
  /// independent session. `opts.convId` / `opts.echoSelf` are forwarded to each `sendTo` — pass the
  /// ORIGINAL conversation id + `echoSelf:true` and `account = self` to drive SIBLING ECHO (fan a
  /// copy to our own other devices on the same conversation, §8). CN: 对 `account` 每个已注册设备在
  /// 各自独立会话上加密并发送 `plaintext`。`opts.convId` / `opts.echoSelf` 透传给每次 `sendTo`——传
  /// 原会话 id + `echoSelf:true` 且 `account = 自己` 即驱动**兄弟设备回显**（在同一会话上向本账户其他
  /// 设备各发一份，§8）。
  async sendToAccount(
    account: string,
    plaintext: Uint8Array,
    opts?: { convId?: string; echoSelf?: boolean },
  ): Promise<MultiDeviceSendResult> {
    const self = this.transport.selfDevice();
    const devices = await this.directory.listDevices(account);
    const sentTo: DeviceId[] = [];
    const skipped: DeviceId[] = [];
    for (const device of devices) {
      if (eqBytes(device, self)) continue; // never fan out to ourselves
      if (!this.transport.hasSession(device)) {
        const bundle = await this.provider.get(account, device);
        if (!bundle) {
          skipped.push(device);
          continue;
        }
        await this.transport.startSession(bundle);
      }
      await this.transport.sendTo(account, device, plaintext, opts);
      sentTo.push(device);
    }
    return { sentTo, skipped };
  }
}
