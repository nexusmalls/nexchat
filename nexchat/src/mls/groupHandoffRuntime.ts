// EN: Track A group sending-authority runtime (design CHAT_MULTIDEVICE_MLS_SYNC §5.2/§7.3). This is the
// APPLICATION-RUNTIME glue that turns the pure handoff crypto (`handoffCoordinator`, `devicePeerKey`,
// `sendingAuthority`) into a live device state machine on the account self-channel `s:<account>`. Under
// the naive shared-leaf escrow model AT MOST ONE device of an account may hold the signing key and
// send/Commit at a time (eliminates §3.1 nonce reuse + concurrent-commit). A device restored from the
// escrow vault is READ-ONLY: it can decrypt group history but must obtain the signing key via an online
// handoff from the account's current authoritative (old primary) device before it can send.
//
// Flow (over `s:<account>` relay control):
//   1. New (read-only) device → `handoff-request` carrying its directory-key-endorsed device peer key.
//   2. Old primary (full client, current authority) → `handoff-grant` = §5 signed receipt (from→to) +
//      the signing-key bundle SEALED to the requester's device peer key. The old primary then re-resolves
//      authority and drops to read-only-for-send (the receipt now names the new device).
//   3. New device opens the sealed bundle with its device peer private key, installs the signing key
//      (becomes a full sender), and re-resolves authority → `primary`.
// Security: forging a request/grant needs `vault_master` (mnemonic) to mint endorsements/receipts, the
// bundle is sealed to a device-specific ECDH key (not derivable from `vault_master`), and the relay only
// fans `s:<account>` to that account's own devices. Replays are rejected by the monotone receipt `seq`
// and the wasm "already has signer" guard.
// CN: 路线 A 群发送权运行时（设计 §5.2/§7.3）。这是把纯交接密码学（`handoffCoordinator`、`devicePeerKey`、
// `sendingAuthority`）变成账户自通道 `s:<account>` 上在线设备态机的**应用层运行时**胶水。朴素共享 leaf 托管下，
// 账户任一时刻**至多一台设备**持签名钥并发送/Commit（消除 §3.1 nonce 重用 + 并发 commit）。由托管 vault 恢复的
// 设备是**只读**的：可解密群历史，但需先经在线交接从账户当前权威（旧主）设备取得签名钥才能发送。
//
// 流程（走 `s:<account>` relay 控制）：
//   1. 新（只读）设备 → `handoff-request`，携带其经目录钥背书的设备对端钥。
//   2. 旧主（完整客户端，当前权威）→ `handoff-grant` = §5 签名收据（from→to）+ 封装给请求方设备对端钥的签名钥
//      bundle。旧主随后重解析权威并降为只读发送（收据已指向新设备）。
//   3. 新设备用其设备对端私钥打开封装 bundle、装入签名钥（成为完整发送者），再重解析权威 → `primary`。
// 安全：伪造请求/授权需 `vault_master`（助记词）来铸造背书/收据；bundle 封装给设备专属 ECDH 钥（不可由
// `vault_master` 派生）；relay 仅把 `s:<account>` 扇给该账户自有设备。重放由单调收据 `seq` 与 wasm「已持签名钥」
// 守卫拒绝。

import { deriveDeviceMode, type DeviceMode } from "@/mls/deviceState";
import {
  endorseDevicePeerKey,
  getOrCreateDevicePeerKey,
  type DevicePeerKey,
} from "@/mls/devicePeerKey";
import {
  openHandoff,
  resolveAuthority,
  sealHandoff,
  type SigningKeyTransfer,
} from "@/mls/handoffCoordinator";
import {
  canSend as authorityCanSend,
  deriveDeviceDirectoryKey,
  type DeviceDirectoryKey,
} from "@/mls/sendingAuthority";
import type { ControlMsg, RelayClient } from "@/relay/relayClient";

/// EN: Engine surface the runtime needs: the §5.2 signing-key transfer + whether this device currently
/// holds a signing key (full client). CN: 运行时所需引擎接口：§5.2 签名钥传输 + 本设备当前是否持签名钥
/// （完整客户端）。
export interface GroupHandoffEngine extends SigningKeyTransfer {
  /// EN: True iff this engine is a FULL client (holds the signing key → can send/export). A read-only
  /// escrow-restored client returns false. CN: 仅当本引擎为**完整**客户端（持签名钥→可发送/导出）时为真；
  /// 只读托管恢复客户端返回 false。
  canExportEscrow(): boolean;
}

export interface GroupHandoffDeps {
  account: string;
  selfDeviceId: string;
  relay: RelayClient;
  engine: GroupHandoffEngine;
  /// EN: `vault_master` root for the device-directory key (§5.2). Null disables the runtime (injector
  /// wallet / mock). CN: 设备目录钥（§5.2）的 `vault_master` 根。null 则停用运行时（注入器钱包/mock）。
  vaultMaster: Uint8Array | null;
  /// EN: Called whenever the send-authority state changes (UI refresh). CN: 发送权状态变化时调用（刷新 UI）。
  onChange?: () => void;
  /// EN: Called once when an online handoff GRANT for THIS device is received and its signing key is
  /// installed (read-only → sender). Drives the "authority received" confirmation. CN: 当本设备的在线
  /// 交接 GRANT 被收到并装入签名钥（只读→可发送）时回调一次。用于「已获得发送权」确认提示。
  onSendAuthorityGranted?: () => void;
}

/// EN: Account-global Track A sending-authority + online-handoff runtime. One instance per unlocked
/// account. CN: 账户级路线 A 发送权 + 在线交接运行时。每个解锁账户一个实例。
export class GroupHandoffRuntime {
  private deps: GroupHandoffDeps | null = null;
  private dir: DeviceDirectoryKey | null = null;
  private peer: DevicePeerKey | null = null;
  private authoritativeDeviceId: string | null = null;
  private restoring = true;
  private controlBound = false;

  /// EN: Start the runtime: derive the device-directory + device peer keys, resolve current authority,
  /// and subscribe to `s:<account>` handoff control. No-op (stays `restoring=false`, mode `primary`)
  /// without a `vault_master` so non-escrow accounts are unaffected. CN: 启动运行时：派生设备目录 + 设备
  /// 对端钥、解析当前权威、订阅 `s:<account>` 交接控制。无 `vault_master` 时为空操作（`restoring=false`、
  /// 态 `primary`），使非托管账户不受影响。
  async start(deps: GroupHandoffDeps): Promise<void> {
    this.deps = deps;
    if (!deps.vaultMaster) {
      this.restoring = false;
      return;
    }
    try {
      this.dir = await deriveDeviceDirectoryKey(deps.vaultMaster);
      this.peer = await getOrCreateDevicePeerKey(deps.account);
    } catch (e) {
      console.warn("[nexchat][handoff] key setup failed (runtime disabled):", e);
      this.restoring = false;
      this.notify();
      return;
    }
    if (!this.controlBound) {
      deps.relay.onControl((msg) => this.onControl(msg));
      this.controlBound = true;
    }
    await this.refreshAuthority();
    this.restoring = false;
    this.notify();
  }

  /// EN: Whether this device may send/Commit to groups right now (authority + signing key, §5.4).
  /// CN: 本设备当前是否可向群发送/Commit（权威 + 签名钥，§5.4）。
  canSend(): boolean {
    if (!this.deps) return true;
    if (!this.dir) return this.deps.engine.canExportEscrow();
    return authorityCanSend({
      localDeviceId: this.deps.selfDeviceId,
      authoritativeDeviceId: this.authoritativeDeviceId,
      hasSigningKey: this.deps.engine.canExportEscrow(),
    });
  }

  /// EN: Current §7.3 device mode for the UI. CN: 供 UI 用的当前 §7.3 设备态。
  mode(): DeviceMode {
    return deriveDeviceMode({ restoring: this.restoring, canSend: this.canSend() });
  }

  /// EN: New-device action: broadcast a `handoff-request` to the account siblings asking for sending
  /// authority. CN: 新设备动作：向账户兄弟设备广播 `handoff-request` 申请发送权。
  async requestSendAuthority(): Promise<void> {
    const deps = this.deps;
    if (!deps || !this.dir || !this.peer) return;
    const endorsement = await endorseDevicePeerKey(
      this.dir,
      deps.selfDeviceId,
      this.peer.publicKeyRaw,
    );
    await deps.relay.sendControl({
      t: "handoff-request",
      convId: `s:${deps.account}`,
      from: deps.selfDeviceId,
      endorsement,
    });
  }

  /// EN: Re-resolve which device currently holds authority from the relay handoff pointer (the §5
  /// receipt), falling back to THIS device as primary only when it is a full client and no receipt
  /// exists yet (§5.1 bootstrap). CN: 从 relay 交接指针（§5 收据）重解析当前权威设备；仅当本设备为完整
  /// 客户端且尚无收据时回退本设备为 primary（§5.1 引导）。
  async refreshAuthority(): Promise<void> {
    const deps = this.deps;
    if (!deps || !this.dir) return;
    const primaryDeviceId = deps.engine.canExportEscrow() ? deps.selfDeviceId : null;
    try {
      this.authoritativeDeviceId = await resolveAuthority({
        account: deps.account,
        dirPublicKey: this.dir.publicKey,
        primaryDeviceId,
      });
    } catch (e) {
      console.warn("[nexchat][handoff] authority resolve failed:", e);
      this.authoritativeDeviceId = primaryDeviceId;
    }
  }

  /// EN: After offline PIN restore installed signing keys (+ optional handoff claim), re-resolve
  /// authority for the UI. CN: 离线 PIN 恢复装入签名钥（+ 可选交接认领）后，重解析权威供 UI 使用。
  async refreshAfterSigningRestored(): Promise<void> {
    await this.refreshAuthority();
    this.notify();
  }

  private async onControl(msg: ControlMsg): Promise<void> {
    const deps = this.deps;
    if (!deps || !this.dir || !this.peer) return;
    if (msg.t === "handoff-request") {
      if (msg.convId !== `s:${deps.account}`) return;
      if (msg.from === deps.selfDeviceId) return;
      // EN: only the device currently holding authority (the old primary full client) may grant.
      // CN: 仅当前持权威的设备（旧主完整客户端）可授权。
      if (!this.canSend()) return;
      try {
        const payload = await sealHandoff({
          account: deps.account,
          dir: this.dir,
          from: deps.selfDeviceId,
          to: msg.from,
          recipientEndorsement: msg.endorsement,
          engine: deps.engine,
        });
        await deps.relay.sendControl({
          t: "handoff-grant",
          convId: `s:${deps.account}`,
          to: msg.from,
          payload,
        });
        // EN: authority has moved to the requester — re-resolve so this device stops sending.
        // CN: 权威已转给请求方——重解析使本设备停止发送。
        await this.refreshAuthority();
        this.notify();
        console.info("[nexchat][handoff] granted send authority", { to: msg.from });
      } catch (e) {
        console.warn("[nexchat][handoff] seal/grant failed:", e);
      }
      return;
    }
    if (msg.t === "handoff-grant") {
      if (msg.convId !== `s:${deps.account}`) return;
      if (msg.to !== deps.selfDeviceId) return;
      if (deps.engine.canExportEscrow()) return; // already a full sender
      try {
        const ok = await openHandoff({
          account: deps.account,
          dirPublicKey: this.dir.publicKey,
          myDeviceId: deps.selfDeviceId,
          myPeerPrivate: this.peer.privateKey,
          payload: msg.payload,
          engine: deps.engine,
        });
        if (ok) {
          await this.refreshAuthority();
          this.notify();
          try {
            deps.onSendAuthorityGranted?.();
          } catch {
            /* UI callback must never break the runtime */
          }
          console.info("[nexchat][handoff] received send authority");
        }
      } catch (e) {
        console.warn("[nexchat][handoff] open/install failed:", e);
      }
    }
  }

  private notify(): void {
    try {
      this.deps?.onChange?.();
    } catch {
      /* UI callback must never break the runtime */
    }
  }

  stop(): void {
    this.deps = null;
    this.dir = null;
    this.peer = null;
    this.authoritativeDeviceId = null;
    this.restoring = true;
    // EN: relay.onControl has no unsubscribe; the bound handler early-returns once `deps` is null.
    // CN: relay.onControl 无退订；解绑后处理器在 `deps` 为 null 时直接返回。
  }
}
