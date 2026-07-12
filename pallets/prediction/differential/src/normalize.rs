//! Normalizers for known Nexus vs upstream semantic deltas.
//! 已知 Nexus 与上游语义差异的归一化器。

use alloc::string::String;
use zeitgeist_primitives::types::{Asset, OutcomeReport};

/// Collateral asset identity after native-name and foreign-width normalization.
/// 原生资产名称与外部资产宽度归一化后的抵押资产标识。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NormalizedAsset {
    /// Upstream `Asset::Ztg` and future `Asset::Native`.
    /// 上游 `Asset::Ztg` 与未来 `Asset::Native`。
    Native,
    /// Nexus `ForeignAsset(u64)` with upstream `u32` ids zero-extended.
    /// 上游 `u32` id 零扩展后的 Nexus `ForeignAsset(u64)`。
    Foreign(u64),
}

/// Normalize a Nexus runtime `Asset` into the differential baseline key space.
/// 将 Nexus runtime `Asset` 归一化到差分基线键空间。
pub fn normalize_asset<MarketId>(asset: Asset<MarketId>) -> Option<NormalizedAsset> {
    match asset {
        Asset::Ztg => Some(NormalizedAsset::Native),
        Asset::ForeignAsset(id) => Some(NormalizedAsset::Foreign(id)),
        _ => None,
    }
}

/// Map an upstream `ForeignAsset(u32)` fixture id to the Nexus semantic `u64` key.
/// 将上游 `ForeignAsset(u32)` fixture id 映射为 Nexus 语义 `u64` 键。
pub fn normalize_upstream_foreign_asset(upstream_id: u32) -> NormalizedAsset {
    NormalizedAsset::Foreign(upstream_id as u64)
}

/// Render an outcome report into a stable cross-tree string.
/// 将结果报告渲染为跨代码树稳定的字符串。
pub fn normalize_outcome(outcome: &OutcomeReport) -> String {
    match outcome {
        OutcomeReport::Categorical(index) => format!("categorical:{index}"),
        OutcomeReport::Scalar(value) => format!("scalar:{value}"),
    }
}

/// Cast an upstream `u64` block number to the Nexus `u32` semantic width.
/// 将上游 `u64` 区块号转换为 Nexus `u32` 语义宽度。
pub fn normalize_block_number(upstream_block: u64) -> u32 {
    upstream_block as u32
}
