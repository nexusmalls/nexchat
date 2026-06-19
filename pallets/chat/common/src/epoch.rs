//! EN: Shared u32 revocation/publication epoch bump helpers. Chat pallets keep
//! orthogonal epoch keys (account capability, inbox, device prekey); only the
//! saturating +1 arithmetic is shared here — not storage layout or events.
//! CN: 共享的 u32 撤销/发布纪元递增辅助函数。各 chat pallet 的 epoch 键正交
//! （账户能力、inbox、设备预密钥）；此处仅共享饱和 +1 算术，不合并 storage 或事件。

/// EN: Increment `epoch` by one (saturating) and return the new value.
/// CN: 将 `epoch` 加一（饱和算术）并返回新值。
pub fn bump_u32_epoch(epoch: &mut u32) -> u32 {
    *epoch = epoch.saturating_add(1);
    *epoch
}

/// EN: Return `epoch + 1` (saturating) without mutating the input.
/// CN: 返回 `epoch + 1`（饱和算术），不修改入参。
pub fn next_u32_epoch(epoch: u32) -> u32 {
    epoch.saturating_add(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_u32_epoch_increments() {
        let mut e = 3u32;
        assert_eq!(bump_u32_epoch(&mut e), 4);
        assert_eq!(e, 4);
    }

    #[test]
    fn bump_u32_epoch_saturates_at_max() {
        let mut e = u32::MAX;
        assert_eq!(bump_u32_epoch(&mut e), u32::MAX);
    }

    #[test]
    fn next_u32_epoch_matches_bump() {
        assert_eq!(next_u32_epoch(9), 10);
        assert_eq!(next_u32_epoch(u32::MAX), u32::MAX);
    }
}
