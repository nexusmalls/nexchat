//! Explicit foreign-collateral whitelist used by prediction mock runtimes.
//! 预测 mock runtime 使用的外部抵押显式白名单。

/// Deterministic whitelist: only the USDX fixture id is allowed.
/// 确定性白名单：仅允许 USDX fixture id。
pub struct MockBaseAssetPolicy;

impl zeitgeist_primitives::traits::PredictionBaseAssetPolicy<u64> for MockBaseAssetPolicy {
    fn is_allowed(asset_id: u64) -> bool {
        asset_id == USDX_ASSET_ID
    }
}

/// Canonical USDX collateral fixture id for mock tests.
/// mock 测试使用的标准 USDX 抵押 fixture id。
pub const USDX_ASSET_ID: u64 = 900_000;

#[cfg(test)]
mod tests {
    use super::*;
    use zeitgeist_primitives::traits::PredictionBaseAssetPolicy;

    #[test]
    fn whitelist_is_explicit_not_universal() {
        assert!(MockBaseAssetPolicy::is_allowed(USDX_ASSET_ID));
        assert!(!MockBaseAssetPolicy::is_allowed(USDX_ASSET_ID + 1));
        assert!(!MockBaseAssetPolicy::is_allowed(1));
    }
}
