// EN: Native test for the MlsClient E2EI device-leaf credential API (§3.9 phase 2):
// signaturePublicKey → setLeafBinding → generateKeyPackage embeds it → keyPackageBinding extracts the
// leaf identity + signature key + binding blob, and validates the KeyPackage on the add path. Also
// asserts an unbound KeyPackage round-trips with an empty binding (backward compatible). The SS58
// signature math lives in TS; this only proves the WASM carries + extracts the blob.
// CN: MlsClient 的 E2EI 设备 leaf 凭证 API 原生测试（§3.9 二阶段）：signaturePublicKey → setLeafBinding →
// generateKeyPackage 嵌入 → keyPackageBinding 提取 leaf identity + 签名钥 + 绑定 blob，并在 add 路径校验
// KeyPackage。并验证未绑定 KeyPackage 往返为空绑定（向后兼容）。SS58 签名运算在 TS；此处仅证明 WASM 承载并提取 blob。

use nexchat_mls::MlsClient;

#[test]
fn leaf_binding_round_trips_through_key_package() {
    let mut client = MlsClient::new("alice#dev1").unwrap();

    let leaf_key = client.signature_public_key().unwrap();
    assert!(!leaf_key.is_empty());

    let blob = b"account-ss58-sig-over(account|device|leafkey)".to_vec();
    client.set_leaf_binding(&blob);

    let kp = client.generate_key_package().unwrap();
    let parsed = client.key_package_binding(&kp).unwrap();

    assert_eq!(parsed.identity, "alice#dev1");
    assert_eq!(parsed.signature_key, leaf_key);
    assert_eq!(parsed.binding, blob);
}

#[test]
fn unbound_key_package_has_empty_binding() {
    let client = MlsClient::new("bob#dev1").unwrap();
    let kp = client.generate_key_package().unwrap();
    let parsed = client.key_package_binding(&kp).unwrap();

    assert_eq!(parsed.identity, "bob#dev1");
    assert!(parsed.binding.is_empty(), "no binding installed → empty");
}

#[test]
fn clearing_the_binding_drops_it_from_new_key_packages() {
    let mut client = MlsClient::new("carol#dev1").unwrap();
    client.set_leaf_binding(b"some-sig");
    assert!(!client.key_package_binding(&client.generate_key_package().unwrap()).unwrap().binding.is_empty());

    client.set_leaf_binding(&[]); // clear
    let parsed = client.key_package_binding(&client.generate_key_package().unwrap()).unwrap();
    assert!(parsed.binding.is_empty(), "cleared binding → empty");
}
