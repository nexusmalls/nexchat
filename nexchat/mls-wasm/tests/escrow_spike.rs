// EN: Track A Gate0 (§7.1) blocking spike — proves the encrypted-state-escrow read path is
// cryptographically feasible on the pinned OpenMLS 0.8.x, WITHOUT escrowing the signature key.
// Hard acceptance items mirrored from CHAT_MULTIDEVICE_MLS_SYNC_DESIGN §7.1:
//   1) export (no signature key) → load on a second device → decrypt current-epoch ciphertext;
//   2) (A1) decrypt same-epoch backlog (generations the snapshot device never consumed) — i.e.
//      the secret-tree root is captured, so earlier generations are still derivable;
//   3) process_message/decrypt + commit catch-up work with NO signature key present.
// CN: 路线 A Gate0（§7.1）阻断 spike —— 在锁定的 OpenMLS 0.8.x 上证明「加密状态托管」读路径在
// 密码学上可行，且**不托管签名私钥**。硬验收项对应设计文档 §7.1：①无签名钥导出→异机装载→解当前
// epoch 密文；②（A1）解同 epoch backlog（快照设备未消费的 generation）—— 即 secret tree 根被捕获、
// 更早 generation 仍可派生；③缺签名钥下 process_message/解密 + commit 补齐均正常。

use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::OpenMlsProvider;
use std::collections::{HashMap, HashSet};
use tls_codec::Serialize as _;

const SUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

struct Party {
    provider: OpenMlsRustCrypto,
    signer: SignatureKeyPair,
    cred: CredentialWithKey,
}

fn party(id: &str) -> Party {
    let provider = OpenMlsRustCrypto::default();
    let signer = SignatureKeyPair::new(SUITE.signature_algorithm()).unwrap();
    signer.store(provider.storage()).unwrap();
    let cred = CredentialWithKey {
        credential: BasicCredential::new(id.as_bytes().to_vec()).into(),
        signature_key: signer.public().into(),
    };
    Party { provider, signer, cred }
}

// EN: Storage keys occupied by a signature keypair (probe a fresh provider with the SAME signer;
// its storage key is deterministic from the public key, so the bytes match the live provider's).
// CN: 签名密钥对在存储中占用的键（用同一 signer 探测一个新 provider；其存储键由公钥确定，故字节
// 与真实 provider 一致）。
fn signer_storage_keys(signer: &SignatureKeyPair) -> HashSet<Vec<u8>> {
    let probe = OpenMlsRustCrypto::default();
    signer.store(probe.storage()).unwrap();
    let keys = probe.storage().values.read().unwrap().keys().cloned().collect();
    keys
}

fn send_app(group: &mut MlsGroup, p: &Party, msg: &[u8]) -> Vec<u8> {
    group
        .create_message(&p.provider, &p.signer, msg)
        .unwrap()
        .tls_serialize_detached()
        .unwrap()
}

// EN: Decrypt an application message using ONLY the provider (no signer) — proves the receiver's
// signature key is not required to read. CN: 仅用 provider（无 signer）解密应用消息——证明读取不需
// 接收方签名钥。
fn decrypt_app(group: &mut MlsGroup, provider: &OpenMlsRustCrypto, wire: &[u8]) -> Vec<u8> {
    let (msg, _) = MlsMessageIn::tls_deserialize_bytes(wire).unwrap();
    let protocol: ProtocolMessage = msg.try_into_protocol_message().unwrap();
    let processed = group.process_message(provider, protocol).unwrap();
    match processed.into_content() {
        ProcessedMessageContent::ApplicationMessage(app) => app.into_bytes(),
        other => panic!("expected application message, got {other:?}"),
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

#[test]
fn escrow_readonly_restore_decrypts_without_signature_key() {
    let alice = party("alice");
    let bob = party("bob");

    // bob publishes a KeyPackage; alice creates the group and adds bob.
    let bob_kp = KeyPackage::builder()
        .build(SUITE, &bob.provider, &bob.signer, bob.cred.clone())
        .unwrap();
    let create_cfg = MlsGroupCreateConfig::builder()
        .ciphersuite(SUITE)
        .use_ratchet_tree_extension(true)
        .build();
    let mut a_group =
        MlsGroup::new(&alice.provider, &alice.signer, &create_cfg, alice.cred.clone()).unwrap();
    let (_commit, welcome, _gi) = a_group
        .add_members(&alice.provider, &alice.signer, &[bob_kp.key_package().clone()])
        .unwrap();
    a_group.merge_pending_commit(&alice.provider).unwrap();
    let group_id = a_group.group_id().clone();

    // bob joins via the Welcome.
    let join_cfg = MlsGroupJoinConfig::builder()
        .use_ratchet_tree_extension(true)
        .build();
    let welcome_in = match MlsMessageIn::tls_deserialize_bytes(&welcome.tls_serialize_detached().unwrap())
        .unwrap()
        .0
        .extract()
    {
        MlsMessageBodyIn::Welcome(w) => w,
        _ => panic!("not a welcome"),
    };
    let mut b_group = StagedWelcome::new_from_welcome(&bob.provider, &join_cfg, welcome_in, None)
        .unwrap()
        .into_group(&bob.provider)
        .unwrap();

    // bob sends backlog at generations 0 and 1; alice (the snapshot device) NEVER consumes them.
    let m_gen0 = send_app(&mut b_group, &bob, b"backlog-gen-0");
    let m_gen1 = send_app(&mut b_group, &bob, b"backlog-gen-1");

    // ── Escrow: alice's storage KV MINUS the signature keypair entries ───────────────────────
    let sig_keys = signer_storage_keys(&alice.signer);
    let full_kv: HashMap<Vec<u8>, Vec<u8>> =
        alice.provider.storage().values.read().unwrap().clone();
    assert!(
        sig_keys.iter().all(|k| full_kv.contains_key(k)),
        "sanity: signature keypair is present in the full storage KV"
    );
    let escrow_kv: HashMap<Vec<u8>, Vec<u8>> =
        full_kv.into_iter().filter(|(k, _)| !sig_keys.contains(k)).collect();
    assert!(
        sig_keys.iter().all(|k| !escrow_kv.contains_key(k)),
        "escrow blob MUST NOT contain the signature private key (Track A §3.2/§3.3)"
    );

    // ── Device 2: read-only restore from the escrow KV (NO signer at all) ─────────────────────
    let dev2 = OpenMlsRustCrypto::default();
    {
        let mut v = dev2.storage().values.write().unwrap();
        for (k, val) in escrow_kv {
            v.insert(k, val);
        }
    }
    let mut d2_group = MlsGroup::load(dev2.storage(), &group_id).unwrap().unwrap();

    // (1)+(2) A1: decrypt same-epoch backlog the snapshot device never consumed, with NO signer.
    assert_eq!(decrypt_app(&mut d2_group, &dev2, &m_gen0), b"backlog-gen-0");
    assert_eq!(decrypt_app(&mut d2_group, &dev2, &m_gen1), b"backlog-gen-1");

    // (3) commit catch-up without a signer: bob self-updates (epoch++), dev2 processes the commit
    // and then decrypts a fresh new-epoch message — all without any signature key on dev2.
    let bundle = b_group
        .self_update(&bob.provider, &bob.signer, LeafNodeParameters::default())
        .unwrap();
    let commit_wire = bundle.commit().tls_serialize_detached().unwrap();
    b_group.merge_pending_commit(&bob.provider).unwrap();
    process_commit(&mut d2_group, &dev2, &commit_wire);
    assert_eq!(d2_group.epoch(), b_group.epoch(), "dev2 caught up to bob's epoch");

    let m_new_epoch = send_app(&mut b_group, &bob, b"post-commit-msg");
    assert_eq!(decrypt_app(&mut d2_group, &dev2, &m_new_epoch), b"post-commit-msg");
}
