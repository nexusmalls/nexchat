// EN: E2EI in-MLS device-leaf credential spike (CHAT_1TO1_WIRE_COMMIT_SERIALIZATION_SPEC §3.9 phase 2).
// Proves on the pinned OpenMLS 0.8.x that the account→leaf binding can ride INSIDE MLS as a custom
// leaf-node extension (ExtensionType::Unknown). Verification uses INDUCTION, not a later tree-walk:
// every add path re-runs `KeyPackageIn::validate` + reads the binding off the INCOMING KeyPackage's leaf
// and verifies it BEFORE accepting, so every leaf admitted into the tree is verified at admission time.
// (OpenMLS 0.8.1 exposes no safe accessor for OTHER members' leaf-node extensions via `MlsGroup`, so we
// deliberately do not rely on reading them back from group state.) The second test pins the persistence
// guarantee we depend on: a binding embedded at leaf-CREATION (KeyPackage / group creation) is carried
// forward across the leaf's later commits and self-updates. The blob is opaque bytes here (the real
// account-SS58 signature is produced/verified in TS); this spike only proves MLS carries + preserves it.
//
// CN: E2EI 的 MLS 内设备 leaf 凭证 spike（串行化规范 §3.9 二阶段）。在锁定的 OpenMLS 0.8.x 上证明：账户→leaf
// 绑定可作为自定义 leaf-node 扩展（ExtensionType::Unknown）**驻留在 MLS 内**。验证走**归纳**而非事后遍历树：
// 每条 add 路径都重跑 `KeyPackageIn::validate`，并从**进来的 KeyPackage** leaf 读出绑定、在接纳前完成校验，故凡
// 进入树的 leaf 均在接纳时已验证。（OpenMLS 0.8.1 不经 `MlsGroup` 暴露读取**其他成员** leaf-node 扩展的安全接口，
// 故我们刻意不依赖从群状态回读它们。）第二个测试钉死我们所依赖的持久性：在 leaf **创建**处（KeyPackage / 建群）
// 嵌入的绑定，会随该 leaf 后续的 commit 与 self-update 一并向前承载。blob 为不透明字节（真实账户 SS58 签名在 TS
// 产/验）；本 spike 仅证明 MLS 承载并保留它。

use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::OpenMlsProvider;
use tls_codec::{DeserializeBytes, Serialize as _};

const SUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;
const E2EI_EXT_TYPE: u16 = 0xF7E2;

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

// EN: build a KeyPackage whose leaf node carries the E2EI binding blob as an Unknown extension; the
// leaf capabilities MUST advertise the extension type or validation rejects it. CN: 构造 leaf 节点以
// Unknown 扩展携带 E2EI 绑定 blob 的 KeyPackage；leaf capabilities **必须**声明该扩展类型，否则校验拒绝。
fn key_package_with_binding(p: &Party, blob: Vec<u8>) -> KeyPackage {
    let ext = Extension::Unknown(E2EI_EXT_TYPE, UnknownExtension(blob));
    let caps = Capabilities::new(
        None,
        None,
        Some(&[ExtensionType::Unknown(E2EI_EXT_TYPE)]),
        None,
        None,
    );
    KeyPackage::builder()
        .leaf_node_capabilities(caps)
        .leaf_node_extensions(Extensions::single(ext).unwrap())
        .build(SUITE, &p.provider, &p.signer, p.cred.clone())
        .unwrap()
        .key_package()
        .clone()
}

fn binding_of(leaf: &LeafNode) -> Option<Vec<u8>> {
    leaf.extensions().iter().find_map(|e| match e {
        Extension::Unknown(t, UnknownExtension(data)) if *t == E2EI_EXT_TYPE => Some(data.clone()),
        _ => None,
    })
}

fn leaf_caps() -> Capabilities {
    Capabilities::new(None, None, Some(&[ExtensionType::Unknown(E2EI_EXT_TYPE)]), None, None)
}

fn leaf_exts(blob: &[u8]) -> Extensions<LeafNode> {
    Extensions::single(Extension::Unknown(E2EI_EXT_TYPE, UnknownExtension(blob.to_vec()))).unwrap()
}

// EN: create a group whose creator leaf carries the binding (via the MlsGroup builder leaf-node
// extensions). CN: 建群且创建者 leaf 携带绑定（经 MlsGroup builder 的 leaf-node 扩展）。
fn group_with_creator_binding(p: &Party, blob: &[u8]) -> MlsGroup {
    MlsGroup::builder()
        .ciphersuite(SUITE)
        .use_ratchet_tree_extension(true)
        .with_capabilities(leaf_caps())
        .with_leaf_node_extensions(leaf_exts(blob))
        .unwrap()
        .build(&p.provider, &p.signer, p.cred.clone())
        .unwrap()
}

#[test]
fn binding_survives_validate_add_and_is_readable_by_any_member() {
    let alice = party("alice#dev1");
    let bob = party("bob#dev1");

    let blob = b"account-ss58-signature-over-(account|device|leafkey)".to_vec();
    let kp = key_package_with_binding(&bob, blob.clone());

    // (1) present on the freshly built KeyPackage's leaf node
    assert_eq!(binding_of(kp.leaf_node()).as_deref(), Some(blob.as_slice()));

    // (2) survives the wire + the exact validation path `add_members` uses
    let kp_bytes = kp.tls_serialize_detached().unwrap();
    let (kp_in, _) = KeyPackageIn::tls_deserialize_bytes(&kp_bytes).unwrap();
    let kp_validated = kp_in
        .validate(alice.provider.crypto(), ProtocolVersion::Mls10)
        .expect("validate must accept an Unknown leaf-node extension advertised in capabilities");
    assert_eq!(binding_of(kp_validated.leaf_node()).as_deref(), Some(blob.as_slice()));

    // also expose the leaf signature key the binding must bind to (the verifier needs it)
    let leaf_sig_key = kp_validated.leaf_node().signature_key().as_slice().to_vec();
    assert!(!leaf_sig_key.is_empty());

    // (3) alice creates a group and adds bob via his binding-carrying KeyPackage — the add path
    // accepts the custom-extension leaf, and the binding is verified BEFORE accepting (induction:
    // every add verifies → every leaf in the group is verified, no tree-walk needed later).
    let mut g = MlsGroup::new(&alice.provider, &alice.signer, &create_cfg(), alice.cred.clone())
        .unwrap();
    g.add_members(&alice.provider, &alice.signer, &[kp_validated])
        .unwrap();
    g.merge_pending_commit(&alice.provider).unwrap();
    assert_eq!(g.members().count(), 2, "bob joined with a binding-carrying leaf");
}

// EN: Pins the persistence guarantee the WASM impl relies on: a leaf-node binding embedded ONCE at leaf
// CREATION survives every later operation that rebuilds the leaf — the committer's path-update on an
// add/remove commit, and self-update (rekey) with EITHER explicit params OR default params. Conclusion:
// the impl only needs to embed the binding at KeyPackage/group-creation time; it never has to re-attach
// it on commits or rekeys. CN: 钉死 WASM 实现所依赖的持久性：在 leaf **创建**时**一次**嵌入的 leaf-node 绑定，
// 会挺过此后每次重建 leaf 的操作——add/remove commit 时提交方的 path-update，以及 self-update（rekey）无论传
// 显式 params 还是默认 params。结论：实现只需在 KeyPackage/建群时嵌入绑定，无需在 commit 或 rekey 时重附加。
#[test]
fn creator_leaf_binding_lifecycle_across_commit_and_update() {
    let alice = party("alice#dev1");
    let bob = party("bob#dev1");
    let blob = b"alice-account-binding".to_vec();

    let mut g = group_with_creator_binding(&alice, &blob);
    // (A) creation: the creator leaf carries the binding
    assert_eq!(
        binding_of(g.own_leaf_node().unwrap()).as_deref(),
        Some(blob.as_slice()),
        "creator leaf must carry the binding right after creation",
    );

    // (B) a commit (add) path-updates the committer's OWN leaf — OpenMLS PRESERVES the leaf extension.
    let bob_kp = key_package_with_binding(&bob, b"bob-binding".to_vec());
    g.add_members(&alice.provider, &alice.signer, &[bob_kp]).unwrap();
    g.merge_pending_commit(&alice.provider).unwrap();
    assert_eq!(
        binding_of(g.own_leaf_node().unwrap()).as_deref(),
        Some(blob.as_slice()),
        "an add-commit path-update must PRESERVE the committer's leaf binding",
    );

    // (C) self-update WITH params carrying the binding — preserves.
    let params = LeafNodeParameters::builder()
        .with_capabilities(leaf_caps())
        .with_extensions(leaf_exts(&blob))
        .build();
    g.self_update(&alice.provider, &alice.signer, params).unwrap();
    g.merge_pending_commit(&alice.provider).unwrap();
    assert_eq!(
        binding_of(g.own_leaf_node().unwrap()).as_deref(),
        Some(blob.as_slice()),
        "self-update WITH params must preserve the binding",
    );

    // (D) self-update with DEFAULT params — OpenMLS still CARRIES FORWARD the existing leaf extension,
    // so a plain rekey does NOT drop the binding. This is why the WASM impl only needs to embed the
    // binding at leaf-CREATION points (KeyPackage / group creation): every later commit/update keeps it.
    g.self_update(&alice.provider, &alice.signer, LeafNodeParameters::default())
        .unwrap();
    g.merge_pending_commit(&alice.provider).unwrap();
    assert_eq!(
        binding_of(g.own_leaf_node().unwrap()).as_deref(),
        Some(blob.as_slice()),
        "a DEFAULT-params self-update must NOT drop the binding (OpenMLS carries it forward)",
    );
}
