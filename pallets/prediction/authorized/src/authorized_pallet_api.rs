use zeitgeist_primitives::traits::{DisputeApi, DisputeMaxWeightApi};

/// Combined dispute API exposed by the authorized resolver.
/// 授权争议解决器暴露的组合争议 API。
pub trait AuthorizedPalletApi: DisputeApi + DisputeMaxWeightApi {}
