// EN: Pinner bookkeeping state files — atomic read/write (`.tmp` -> rename), JSON + trailing
// newline. Byte-compatible with the JS pinners' readStateFile/writeStateFile.
// CN: pinner 记账 state 文件——原子读写（`.tmp` -> rename），JSON + 末尾换行。与 JS pinner
// 的 readStateFile/writeStateFile 兼容。

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// EN: Parse a state file; None on missing/corrupt (parity with JS). CN: 读取 state 文件。
pub fn read_state_file<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// EN: Atomically write a state file (`.tmp` -> rename). CN: 原子写入 state 文件。
pub fn write_state_file<T: Serialize>(path: &Path, state: &T) -> io::Result<()> {
    let tmp = PathBuf::from(format!("{}.tmp", path.display()));
    let body = format!(
        "{}\n",
        serde_json::to_string(state).map_err(io::Error::other)?
    );
    fs::write(&tmp, body)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::OnlyAddState;

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("relay-chain-pinner-state.json");
        let mut st = OnlyAddState {
            v: 1,
            ..Default::default()
        };
        st.requested
            .insert("bafyc".into(), serde_json::json!({ "at": 1, "size": 9 }));
        write_state_file(&path, &st).unwrap();

        let back: OnlyAddState = read_state_file(&path).unwrap();
        assert_eq!(st, back);
        assert!(read_state_file::<OnlyAddState>(&dir.path().join("missing.json")).is_none());
    }
}
