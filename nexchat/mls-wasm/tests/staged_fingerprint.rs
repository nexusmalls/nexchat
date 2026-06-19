// EN: G3b — staged-commit post-commit fingerprint (CHAT_GROUP_WIREIFY_DESIGN §7.2). A group Wire
// device op must submit the chain `commit(new_tree_hash, new_transcript_hash)` with the POST-commit
// commitments BEFORE the `expected_epoch` CAS verdict, i.e. WITHOUT a speculative merge.
// `stagedCommitFingerprint` reads the pending `StagedCommit::group_context()` — exactly the context
// `merge` installs — so the value is the true post-commit commitment that every member converging on
// this commit shares.
//
// Native coverage drives this through the public `MlsClient` API. Multi-member adds need JS
// `Uint8Array` KeyPackages (not constructible on the native target — see `group_wire_spike.rs` for the
// raw-OpenMLS multi-device harness), so here we exercise the primitive with `selfUpdateStaged` on a
// single-leaf group, which produces a pending commit with no KeyPackage args. These tests pin: the
// staged epoch is pre+1 (post-commit), staging does not advance the live epoch, merging lands on the
// staged epoch, the hashes are 32 bytes, and reading with nothing staged errors.
//
// CN: G3b——暂存 commit 的后置指纹（设计 §7.2）。群 Wire 设备操作须在 `expected_epoch` CAS 裁决**之前**（即**不**
// 投机合并）以**后置**承诺提交链上 `commit(new_tree_hash, new_transcript_hash)`。`stagedCommitFingerprint`
// 读 pending `StagedCommit::group_context()`——正是 `merge` 安装的 context——故其值即所有收敛到此 commit 的成员
// 共享的真实后置承诺。
//
// 原生覆盖经公开 `MlsClient` API 驱动。多成员 add 需 JS `Uint8Array` KeyPackage（原生目标无法构造——多设备
// 原始 OpenMLS harness 见 `group_wire_spike.rs`），故此处用单 leaf 群的 `selfUpdateStaged` 触发 pending commit
// （无 KeyPackage 入参）。本测试钉住：暂存 epoch 为 pre+1（后置）、暂存不推进实时 epoch、合并落到暂存 epoch、
// 哈希 32 字节、无暂存读取报错。

use nexchat_mls::MlsClient;

const A: &str = "g:1";

#[test]
fn staged_fingerprint_is_post_commit_and_lands_on_merge() {
    let mut alice = MlsClient::new("alice#a").expect("new alice");
    alice.create_group(A).expect("create");
    let pre_epoch = alice.epoch(A).expect("epoch"); // epoch 0

    // STAGE a self-update (rekey) WITHOUT merging, then read the staged (post-commit) fingerprint.
    alice.self_update_staged(A).expect("stage self-update");
    let sfp = alice.staged_commit_fingerprint(A).expect("staged fp");

    // staging does NOT advance the live epoch; staged fp is the POST-commit epoch (pre + 1).
    assert_eq!(alice.epoch(A).unwrap(), pre_epoch, "staging does not advance live epoch");
    assert_eq!(sfp.epoch, pre_epoch + 1, "staged fp epoch is post-commit");
    assert_eq!(sfp.tree_hash.len(), 32, "tree_hash is 32B");
    assert_eq!(sfp.transcript_hash.len(), 32, "transcript_hash is 32B");

    // committer merges and lands on exactly the staged epoch.
    alice.merge_pending(A).expect("merge");
    assert_eq!(alice.epoch(A).unwrap(), sfp.epoch, "merge lands on staged epoch");
}

#[test]
fn staged_fingerprint_advances_each_staged_commit() {
    let mut alice = MlsClient::new("alice#a").expect("new alice");
    alice.create_group(A).expect("create");

    // first staged commit → epoch 1
    alice.self_update_staged(A).expect("stage #1");
    let fp1 = alice.staged_commit_fingerprint(A).expect("fp #1");
    assert_eq!(fp1.epoch, 1);
    alice.merge_pending(A).expect("merge #1");

    // second staged commit → epoch 2, with a DIFFERENT (advanced) commitment
    alice.self_update_staged(A).expect("stage #2");
    let fp2 = alice.staged_commit_fingerprint(A).expect("fp #2");
    assert_eq!(fp2.epoch, 2);
    assert_ne!(fp1.tree_hash, fp2.tree_hash, "tree_hash advances per epoch");
    assert_ne!(
        fp1.transcript_hash, fp2.transcript_hash,
        "transcript_hash advances per epoch"
    );
}

// NOTE: the no-pending-commit ERROR path is intentionally NOT covered here: constructing the
// `JsError` it returns calls a wasm-bindgen import that panics on the native target. That path is
// covered by the TS engine wrapper test (`stagedCommitFingerprint` throws when nothing is staged).
// CN: 无 pending commit 的**错误**路径不在此覆盖：其返回的 `JsError` 构造会调用 wasm-bindgen 导入，原生目标会
// panic。该路径由 TS 引擎包装测试覆盖（无暂存时 `stagedCommitFingerprint` 抛错）。
