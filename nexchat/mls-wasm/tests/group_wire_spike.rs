// EN: Group Wire-ification spike (G0, blocking). Proves on the pinned OpenMLS 0.8.x — WITHOUT any
// virtual-clients feature (NOT gated by Gate-B) — that a real GROUP (>=3 accounts) can run the
// per-device-leaf model that today only the 1:1 path uses:
//   S1: one account holds TWO leaves (two devices) in a >=3-account group, bound to ONE identity,
//       the second device joining the EXISTING group via add_device + Welcome.
//   S2: both of that account's devices send concurrently at the SAME epoch with no AEAD nonce
//       reuse; every other member AND the sibling device read both messages.
//   S3: removing one device gives per-leaf PCS — the removed device cannot decrypt new group
//       messages, with no mnemonic rotation.
// This is the wasm half of CHAT_GROUP_WIREIFY_DESIGN.md §11.1 (S1-S3). The chain half (empty-delta
// rekey commit) is already proven by pallet-chat-group's `same_account_empty_delta_commit_rekey_is_allowed`.
// Mirrors the harness of `hybrid_spike.rs`.
//
// CN: 群 Wire 化 spike（G0，阻断项）。在锁定的 OpenMLS 0.8.x 上证明——**不依赖任何 virtual-clients
// feature，即不受 Gate-B 阻断**：真实**群**（≥3 账户）可跑通今天仅 1:1 路径使用的每设备 leaf 模型：
//   S1：同一账户在 ≥3 账户群里持**两个 leaf**（两台设备）、绑**同一** identity；第二台设备经
//       add_device + Welcome 加入**已存在**的群。
//   S2：该账户两设备在**同一 epoch** 并发发送，不触发 AEAD nonce 重用；其它每个成员**与兄弟设备**都能读两条。
//   S3：移除其中一台设备得到按 leaf 的 PCS——被移除设备解不出群新消息，且无需助记词轮换。
// 这是 CHAT_GROUP_WIREIFY_DESIGN.md §11.1（S1-S3）的 wasm 半边。链侧半边（空-delta 推进 rekey commit）
// 已由 pallet-chat-group 的 `same_account_empty_delta_commit_rekey_is_allowed` 证明。
// 复用 `hybrid_spike.rs` 的 harness。

use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::OpenMlsProvider;
use tls_codec::Serialize as _;

const SUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

// EN: A single device = its own provider + signer. Two devices of the SAME account are two `Party`s
// that share the same `identity` bytes (minimal E2EI binding model: distinct leaves, one identity).
// CN: 单台设备 = 独立 provider + signer。同账户两设备 = 两个共享同一 `identity` 字节的 `Party`
// （最小 E2EI 绑定模型：多 leaf、同身份）。
struct Party {
    provider: OpenMlsRustCrypto,
    signer: SignatureKeyPair,
    cred: CredentialWithKey,
}

fn party(identity: &str) -> Party {
    let provider = OpenMlsRustCrypto::default();
    let signer = SignatureKeyPair::new(SUITE.signature_algorithm()).unwrap();
    signer.store(provider.storage()).unwrap();
    let cred = CredentialWithKey {
        credential: BasicCredential::new(identity.as_bytes().to_vec()).into(),
        signature_key: signer.public().into(),
    };
    Party { provider, signer, cred }
}

fn create_cfg() -> MlsGroupCreateConfig {
    MlsGroupCreateConfig::builder()
        .ciphersuite(SUITE)
        .use_ratchet_tree_extension(true)
        .build()
}

fn join_cfg() -> MlsGroupJoinConfig {
    MlsGroupJoinConfig::builder().use_ratchet_tree_extension(true).build()
}

fn key_package(p: &Party) -> KeyPackage {
    KeyPackage::builder()
        .build(SUITE, &p.provider, &p.signer, p.cred.clone())
        .unwrap()
        .key_package()
        .clone()
}

fn new_group(owner: &Party) -> MlsGroup {
    MlsGroup::new(&owner.provider, &owner.signer, &create_cfg(), owner.cred.clone()).unwrap()
}

// EN: `committer` adds `kp`; returns serialized commit + welcome and merges the pending commit.
// CN: `committer` 加入 `kp`；返回序列化 commit + welcome 并合并待定 commit。
fn add_member(group: &mut MlsGroup, committer: &Party, kp: KeyPackage) -> (Vec<u8>, MlsMessageOut) {
    let (commit, welcome, _gi) = group.add_members(&committer.provider, &committer.signer, &[kp]).unwrap();
    group.merge_pending_commit(&committer.provider).unwrap();
    (commit.tls_serialize_detached().unwrap(), welcome)
}

fn join_from_welcome(joiner: &Party, welcome: &MlsMessageOut) -> MlsGroup {
    let welcome_in = match MlsMessageIn::tls_deserialize_bytes(&welcome.tls_serialize_detached().unwrap())
        .unwrap()
        .0
        .extract()
    {
        MlsMessageBodyIn::Welcome(w) => w,
        _ => panic!("not a welcome"),
    };
    StagedWelcome::new_from_welcome(&joiner.provider, &join_cfg(), welcome_in, None)
        .unwrap()
        .into_group(&joiner.provider)
        .unwrap()
}

fn send_app(group: &mut MlsGroup, p: &Party, msg: &[u8]) -> Vec<u8> {
    group
        .create_message(&p.provider, &p.signer, msg)
        .unwrap()
        .tls_serialize_detached()
        .unwrap()
}

fn decrypt_app(group: &mut MlsGroup, provider: &OpenMlsRustCrypto, wire: &[u8]) -> Vec<u8> {
    let (msg, _) = MlsMessageIn::tls_deserialize_bytes(wire).unwrap();
    let protocol: ProtocolMessage = msg.try_into_protocol_message().unwrap();
    let processed = group.process_message(provider, protocol).unwrap();
    match processed.into_content() {
        ProcessedMessageContent::ApplicationMessage(app) => app.into_bytes(),
        other => panic!("expected application message, got {other:?}"),
    }
}

// EN: Like `decrypt_app` but returns Err instead of panicking — for the PCS negative assertion.
// CN: 类似 `decrypt_app`，但返回 Err 而非 panic——用于 PCS 负向断言。
fn try_decrypt_app(group: &mut MlsGroup, provider: &OpenMlsRustCrypto, wire: &[u8]) -> Result<Vec<u8>, String> {
    let (msg, _) = MlsMessageIn::tls_deserialize_bytes(wire).map_err(|e| e.to_string())?;
    let protocol: ProtocolMessage = msg.try_into_protocol_message().map_err(|e| e.to_string())?;
    let processed = group.process_message(provider, protocol).map_err(|e| format!("{e:?}"))?;
    match processed.into_content() {
        ProcessedMessageContent::ApplicationMessage(app) => Ok(app.into_bytes()),
        other => Err(format!("not an application message: {other:?}")),
    }
}

fn process_commit(group: &mut MlsGroup, provider: &OpenMlsRustCrypto, wire: &[u8]) {
    let (msg, _) = MlsMessageIn::tls_deserialize_bytes(wire).unwrap();
    let protocol: ProtocolMessage = msg.try_into_protocol_message().unwrap();
    let processed = group.process_message(provider, protocol).unwrap();
    match processed.into_content() {
        ProcessedMessageContent::StagedCommitMessage(staged) => {
            group.merge_staged_commit(provider, *staged).unwrap();
        }
        other => panic!("expected commit, got {other:?}"),
    }
}

fn credential_identity(c: &Credential) -> String {
    if c.credential_type() == CredentialType::Basic {
        String::from_utf8_lossy(c.serialized_content()).into_owned()
    } else {
        String::new()
    }
}

// EN: Count leaves whose credential identity == `identity`. CN: 统计 identity 等于该值的 leaf 数。
fn count_identity(group: &MlsGroup, identity: &str) -> usize {
    group.members().filter(|m| credential_identity(&m.credential) == identity).count()
}

// EN: Build a 3-account group (alice device A as creator) and graft alice's SECOND device (alice_b)
// into the EXISTING group via add_device + Welcome. Returns every group view so tests can drive
// concurrent sends / removals. CN: 建一个 3 账户群（alice 设备 A 为建者），经 add_device + Welcome
// 把 alice 的**第二台设备**（alice_b）并入**已存在**的群。返回所有群视图，供并发发送 / 移除用例驱动。
struct GroupWorld {
    alice_a: Party,
    alice_b: Party,
    bob: Party,
    carol: Party,
    a_group: MlsGroup,
    b_group: MlsGroup,
    bob_group: MlsGroup,
    carol_group: MlsGroup,
}

fn build_group_with_second_device() -> GroupWorld {
    let alice_a = party("alice");
    let alice_b = party("alice");
    let bob = party("bob");
    let carol = party("carol");

    // Group baseline: alice_a creates and adds bob + carol (>=3 accounts, mirrors the on-chain shape).
    let mut a_group = new_group(&alice_a);
    let (_c_bob, w_bob) = add_member(&mut a_group, &alice_a, key_package(&bob));
    let mut bob_group = join_from_welcome(&bob, &w_bob);

    let (c_carol, w_carol) = add_member(&mut a_group, &alice_a, key_package(&carol));
    process_commit(&mut bob_group, &bob.provider, &c_carol); // bob follows carol's add
    let mut carol_group = join_from_welcome(&carol, &w_carol);

    // S1 (core): alice's SECOND device joins the EXISTING group. alice_a (coordinator device, CD)
    // commits add_device(alice_b's KeyPackage); every other member follows the commit; alice_b joins
    // from the Welcome delivered over s:<account> (modelled here as direct Welcome handoff).
    let (commit_add_b, w_ab) = add_member(&mut a_group, &alice_a, key_package(&alice_b));
    process_commit(&mut bob_group, &bob.provider, &commit_add_b);
    process_commit(&mut carol_group, &carol.provider, &commit_add_b);
    let b_group = join_from_welcome(&alice_b, &w_ab);

    GroupWorld { alice_a, alice_b, bob, carol, a_group, b_group, bob_group, carol_group }
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// S1 — same account holds two leaves in a >=3-account group, bound to one identity
// ─────────────────────────────────────────────────────────────────────────────────────────────
#[test]
fn s1_group_second_device_joins_existing_group_as_distinct_leaf() {
    let w = build_group_with_second_device();

    // All four views agree on the epoch after the device add.
    assert_eq!(w.a_group.epoch(), w.b_group.epoch());
    assert_eq!(w.a_group.epoch(), w.bob_group.epoch());
    assert_eq!(w.a_group.epoch(), w.carol_group.epoch());

    // 4 leaves total: alice_a + alice_b + bob + carol.
    assert_eq!(w.a_group.members().count(), 4, "alice_a + alice_b + bob + carol");

    // alice owns TWO leaves under ONE identity; bob/carol own one each.
    assert_eq!(count_identity(&w.a_group, "alice"), 2, "two alice devices = two leaves, one identity");
    assert_eq!(count_identity(&w.a_group, "bob"), 1);
    assert_eq!(count_identity(&w.a_group, "carol"), 1);

    // Other members see alice's device count too (the explicit privacy cost vs Route B).
    assert_eq!(count_identity(&w.bob_group, "alice"), 2, "peers see alice's device count");
    assert_eq!(count_identity(&w.carol_group, "alice"), 2);
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// S2 — concurrent send from two devices at the same epoch, no nonce reuse, everyone reads both
// ─────────────────────────────────────────────────────────────────────────────────────────────
#[test]
fn s2_two_devices_send_concurrently_without_nonce_reuse() {
    let mut w = build_group_with_second_device();
    let start_epoch = w.a_group.epoch();

    // Both alice devices send N messages at the SAME starting epoch (no commit in between). Distinct
    // leaves → distinct secret-tree branches → no (key,nonce) reuse by MLS construction.
    const N: usize = 4;
    let mut wires_a = Vec::new();
    let mut wires_b = Vec::new();
    for i in 0..N {
        wires_a.push(send_app(&mut w.a_group, &w.alice_a, format!("A-{i}").as_bytes()));
        wires_b.push(send_app(&mut w.b_group, &w.alice_b, format!("B-{i}").as_bytes()));
    }

    // Epoch did NOT advance — these really are same-epoch concurrent application messages.
    assert_eq!(w.a_group.epoch(), start_epoch, "application messages must not advance the epoch");
    assert_eq!(w.b_group.epoch(), start_epoch);

    // No two ciphertexts collide (proxy for no (key,nonce) reuse across the two leaves).
    let mut all: Vec<&Vec<u8>> = wires_a.iter().chain(wires_b.iter()).collect();
    let total = all.len();
    all.sort();
    all.dedup();
    assert_eq!(all.len(), total, "all ciphertexts must be distinct (no nonce/key reuse)");

    // Every OTHER member decrypts both devices' streams.
    for i in 0..N {
        assert_eq!(decrypt_app(&mut w.bob_group, &w.bob.provider, &wires_a[i]), format!("A-{i}").as_bytes());
        assert_eq!(decrypt_app(&mut w.bob_group, &w.bob.provider, &wires_b[i]), format!("B-{i}").as_bytes());
        assert_eq!(decrypt_app(&mut w.carol_group, &w.carol.provider, &wires_a[i]), format!("A-{i}").as_bytes());
        assert_eq!(decrypt_app(&mut w.carol_group, &w.carol.provider, &wires_b[i]), format!("B-{i}").as_bytes());
    }

    // Each alice device reads the SIBLING device's stream (different members → decryptable).
    for i in 0..N {
        assert_eq!(decrypt_app(&mut w.b_group, &w.alice_b.provider, &wires_a[i]), format!("A-{i}").as_bytes());
        assert_eq!(decrypt_app(&mut w.a_group, &w.alice_a.provider, &wires_b[i]), format!("B-{i}").as_bytes());
    }
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// S3 — removing a device gives per-leaf PCS (removed device cannot read new group messages)
// ─────────────────────────────────────────────────────────────────────────────────────────────
#[test]
fn s3_remove_device_gives_per_leaf_pcs() {
    let mut w = build_group_with_second_device();

    // alice_a (CD) removes alice_b's leaf and commits; all remaining members follow.
    let b_leaf = w
        .a_group
        .members()
        .find(|m| credential_identity(&m.credential) == "alice" && m.index != w.a_group.own_leaf_index())
        .map(|m| m.index)
        .expect("alice_b leaf present");
    let (remove_commit, _w, _gi) =
        w.a_group.remove_members(&w.alice_a.provider, &w.alice_a.signer, &[b_leaf]).unwrap();
    w.a_group.merge_pending_commit(&w.alice_a.provider).unwrap();
    let remove_wire = remove_commit.tls_serialize_detached().unwrap();
    process_commit(&mut w.bob_group, &w.bob.provider, &remove_wire);
    process_commit(&mut w.carol_group, &w.carol.provider, &remove_wire);

    // alice now has ONE leaf in the group; bob/carol unaffected.
    assert_eq!(count_identity(&w.a_group, "alice"), 1, "removed device's leaf is gone");

    // New-epoch message: every remaining member reads it; the removed device does NOT (per-leaf PCS).
    let post = send_app(&mut w.a_group, &w.alice_a, b"after-device-removal");
    assert_eq!(decrypt_app(&mut w.bob_group, &w.bob.provider, &post), b"after-device-removal");
    assert_eq!(decrypt_app(&mut w.carol_group, &w.carol.provider, &post), b"after-device-removal");
    assert!(
        try_decrypt_app(&mut w.b_group, &w.alice_b.provider, &post).is_err(),
        "removed device must NOT decrypt post-removal group messages (per-leaf PCS)"
    );
}
