// EN: Native tests for account + session pickle round-trip (design §17.2 at-rest encryption
// is TS-only; pickles here are plaintext JSON as exported by WASM).
// CN: 账户 + 会话 pickle 往返原生测试（§17.2 落盘加密在 TS；此处 pickle 为 WASM 导出的明文 JSON）。

use nexchat_dr::DrClient;

fn peer_hex(client: &DrClient) -> String {
    client.identity_key()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn establish_session(alice: &mut DrClient, bob: &mut DrClient) {
    let bob_hex = peer_hex(bob);
    let alice_hex = peer_hex(alice);
    bob.generate_fallback_key();
    bob.generate_one_time_keys(1);
    let prekey = bob.one_time_keys()[..32].to_vec();
    alice
        .create_outbound_session(&bob_hex, &bob.identity_key(), &prekey)
        .expect("outbound");
    let wire = alice.encrypt(&bob_hex, b"hello").expect("encrypt");
    let body = &wire[1..];
    bob.create_inbound_session(&alice_hex, body)
        .expect("inbound");
}

#[test]
fn account_pickle_restores_identity_key() {
    let client = DrClient::new();
    let ik = client.identity_key();
    let pickle = client.pickle().expect("pickle account");
    let restored = DrClient::restore(&pickle).expect("restore account");
    assert_eq!(restored.identity_key(), ik);
    assert!(!restored.has_session("any"));
}

#[test]
fn session_pickle_survives_restart_and_decrypts() {
    let mut alice = DrClient::new();
    let mut bob = DrClient::new();
    establish_session(&mut alice, &mut bob);

    let bob_hex = peer_hex(&bob);
    let alice_hex = peer_hex(&alice);
    let account_pickle = alice.pickle().expect("account pickle");
    let session_pickle = alice.pickle_session(&bob_hex).expect("session pickle");

    let mut alice2 = DrClient::restore(&account_pickle).expect("restore account");
    alice2
        .load_session(&bob_hex, &session_pickle)
        .expect("load session");
    assert!(alice2.has_session(&bob_hex));

    let reply = bob.encrypt(&alice_hex, b"welcome back").expect("reply");
    let plain = alice2
        .decrypt(&bob_hex, reply[0], &reply[1..])
        .expect("decrypt after restore");
    assert_eq!(plain, b"welcome back");
}
