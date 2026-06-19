// EN: Native test for the read-only member roster API (design §8 / spec §3.9): memberIdentities returns
// the leaf credential identities (`account#deviceId`) of every member in the bound group, and an empty
// list when no group is bound. Multi-leaf rosters are asserted in the TS engine round-trip test (which
// can build Uint8Array KeyPackages); this native smoke test pins the single-leaf + no-group semantics.
// CN: 只读成员名册 API 原生测试（设计 §8 / 规范 §3.9）：memberIdentities 返回绑定群每个成员的 leaf 凭证身份
// （`account#deviceId`），无群时返回空列表。多 leaf 名册在 TS 引擎往返测试中断言（那里能构造 Uint8Array
// KeyPackage）；本原生冒烟测试钉死单 leaf + 无群语义。

use nexchat_mls::MlsClient;

#[test]
fn member_identities_lists_the_creator_leaf() {
    let mut client = MlsClient::new("alice#dev1").unwrap();
    client.create_group("d:alice:bob").unwrap();

    let roster = client.member_identities("d:alice:bob");
    assert_eq!(roster, vec!["alice#dev1".to_string()]);
}

#[test]
fn member_identities_is_empty_for_an_unbound_conversation() {
    let client = MlsClient::new("bob#dev1").unwrap();
    assert!(client.member_identities("d:alice:bob").is_empty());
}
