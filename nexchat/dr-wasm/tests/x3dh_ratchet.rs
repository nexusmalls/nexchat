// EN: Native smoke tests for X3DH + Double Ratchet via `DrClient` (design §17.1).
// Pins OPK path, SPK fallback, and multi-round ratchet without going through JS/WASM.
// CN: `DrClient` 的 X3DH + 双棘轮原生冒烟测试（设计 §17.1）。钉死 OPK 路径、SPK 回退与多轮
// ratchet，不经 JS/WASM。

use nexchat_dr::DrClient;

fn peer_hex(client: &DrClient) -> String {
    client.identity_key()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn prekey_for_outbound(bob: &mut DrClient, use_opk: bool) -> Vec<u8> {
    bob.generate_fallback_key();
    if use_opk {
        bob.generate_one_time_keys(3);
        let keys = bob.one_time_keys();
        assert!(keys.len() >= 32, "expected at least one OPK");
        keys[..32].to_vec()
    } else {
        bob.fallback_key().expect("SPK fallback key missing")
    }
}

fn split_wire(out: &[u8]) -> (u8, &[u8]) {
    assert!(!out.is_empty(), "wire output must include msg_type byte");
    (out[0], &out[1..])
}

#[test]
fn x3dh_with_opk_and_ratchet_roundtrip() {
    let mut alice = DrClient::new();
    let mut bob = DrClient::new();
    let bob_hex = peer_hex(&bob);
    let alice_hex = peer_hex(&alice);

    let bob_ik = bob.identity_key();
    let prekey = prekey_for_outbound(&mut bob, true);
    alice
        .create_outbound_session(&bob_hex, &bob_ik, &prekey)
        .expect("outbound session");

    let wire1 = alice
        .encrypt(&bob_hex, b"hello bob")
        .expect("first encrypt");
    let (_, body1) = split_wire(&wire1);

    let inbound = bob
        .create_inbound_session(&alice_hex, body1)
        .expect("inbound session");
    assert_eq!(inbound.plaintext, b"hello bob");
    assert_eq!(inbound.identity_key, alice.identity_key());

    let reply = bob.encrypt(&alice_hex, b"hi alice").expect("reply");
    let (msg_type, body) = split_wire(&reply);
    assert_eq!(msg_type, 1, "post-init messages must be Normal/Msg");
    let plain = alice.decrypt(&bob_hex, msg_type, body).expect("decrypt reply");
    assert_eq!(plain, b"hi alice");

    for (i, msg) in [b"msg2", b"msg3", b"msg4"].into_iter().enumerate() {
        let out = alice.encrypt(&bob_hex, msg).expect("alice encrypt");
        let (t, b) = split_wire(&out);
        let got = bob.decrypt(&alice_hex, t, b).expect("bob decrypt");
        assert_eq!(got, msg, "round {i}");
    }
}

#[test]
fn x3dh_falls_back_to_spk_when_no_opk() {
    let mut alice = DrClient::new();
    let mut bob = DrClient::new();
    let bob_hex = peer_hex(&bob);
    let alice_hex = peer_hex(&alice);

    let bob_ik = bob.identity_key();
    let spk = prekey_for_outbound(&mut bob, false);
    alice
        .create_outbound_session(&bob_hex, &bob_ik, &spk)
        .expect("outbound via SPK");

    let wire1 = alice.encrypt(&bob_hex, b"via spk").expect("encrypt");
    let (_, body1) = split_wire(&wire1);
    let inbound = bob
        .create_inbound_session(&alice_hex, body1)
        .expect("inbound");
    assert_eq!(inbound.plaintext, b"via spk");

    let reply = bob.encrypt(&alice_hex, b"ack").expect("reply");
    let (t, b) = split_wire(&reply);
    assert_eq!(
        alice.decrypt(&bob_hex, t, b).expect("decrypt"),
        b"ack"
    );
}
