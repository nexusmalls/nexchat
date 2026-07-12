// Copyright (C) Nexus contributors
// SPDX-License-Identifier: MIT-0
//
//! SCALE golden vectors for Nexus prediction primitives.
//! Nexus 预测 primitives 的 SCALE golden 向量。

#[cfg(test)]
mod tests {
    use crate::{
        asset::{
            foreign_asset_from_upstream, foreign_asset_from_upstream_id, Asset, ScalarPosition,
        },
        types::{BlockNumber, CategoryIndex, CombinatorialId, MarketId, PoolId},
    };
    use parity_scale_codec::{Decode, Encode};

    const USDX_UPSTREAM_FIXTURE: u32 = 900_000;
    const USDX_NEXUS_ID: u64 = 900_000;

    #[test]
    fn asset_variant_discriminants_are_locked() {
        assert_eq!(Asset::<MarketId>::CategoricalOutcome(0, 0).encode()[0], 0);
        assert_eq!(
            Asset::<MarketId>::ScalarOutcome(0, ScalarPosition::Long).encode()[0],
            1
        );
        assert_eq!(Asset::<MarketId>::CombinatorialOutcomeLegacy.encode()[0], 2);
        assert_eq!(Asset::<MarketId>::PoolShare(0).encode()[0], 3);
        assert_eq!(Asset::<MarketId>::Ztg.encode(), [4]);
        assert_eq!(
            Asset::<MarketId>::ForeignAsset(USDX_NEXUS_ID).encode()[0],
            5
        );
        assert_eq!(Asset::<MarketId>::ParimutuelShare(0, 0).encode()[0], 6);
        assert_eq!(
            Asset::<MarketId>::CombinatorialToken([0u8; 32]).encode()[0],
            7
        );
    }

    #[test]
    fn ztg_encoding_is_stable() {
        assert_eq!(Asset::<MarketId>::Ztg.encode(), [4]);
        assert_eq!(
            Asset::<MarketId>::Ztg,
            Asset::<MarketId>::decode(&mut &[4][..]).expect("ztg decodes")
        );
    }

    #[test]
    fn foreign_asset_upstream_u32_fixture_zero_extends_to_u64() {
        assert_eq!(
            foreign_asset_from_upstream_id(USDX_UPSTREAM_FIXTURE),
            USDX_NEXUS_ID
        );
        assert_eq!(
            foreign_asset_from_upstream::<MarketId>(USDX_UPSTREAM_FIXTURE),
            Asset::ForeignAsset(USDX_NEXUS_ID)
        );
    }

    #[test]
    fn foreign_asset_u64_encoding_differs_from_upstream_u32_fixture() {
        let upstream_bytes = {
            let mut bytes = vec![5u8];
            bytes.extend_from_slice(&USDX_UPSTREAM_FIXTURE.encode());
            bytes
        };
        let nexus_bytes = Asset::<MarketId>::ForeignAsset(USDX_NEXUS_ID).encode();
        assert_ne!(upstream_bytes, nexus_bytes);
        assert_eq!(
            Asset::<MarketId>::ForeignAsset(USDX_NEXUS_ID),
            Asset::<MarketId>::decode(&mut &nexus_bytes[..]).expect("foreign asset decodes")
        );
    }

    #[test]
    fn foreign_asset_accepts_nexus_only_u64_ids() {
        let large_id = (u32::MAX as u64) + 1;
        let encoded = Asset::<MarketId>::ForeignAsset(large_id).encode();
        assert_eq!(
            Asset::<MarketId>::ForeignAsset(large_id),
            Asset::<MarketId>::decode(&mut &encoded[..]).expect("large foreign asset decodes")
        );
    }

    #[test]
    fn categorical_outcome_roundtrip() {
        let asset = Asset::<MarketId>::CategoricalOutcome(42, 3);
        let encoded = asset.encode();
        assert_eq!(
            asset,
            Asset::<MarketId>::decode(&mut &encoded[..]).expect("categorical outcome decodes")
        );
    }

    #[test]
    fn block_number_alias_is_u32() {
        let block: BlockNumber = 1_000_000;
        let encoded = block.encode();
        assert_eq!(encoded.len(), 4);
        assert_eq!(BlockNumber::decode(&mut &encoded[..]).unwrap(), block);
    }

    #[test]
    fn scalar_position_encoding_is_stable() {
        assert_eq!(ScalarPosition::Long.encode(), [0]);
        assert_eq!(ScalarPosition::Short.encode(), [1]);
    }

    #[test]
    fn pool_share_and_combinatorial_token_roundtrip() {
        let pool = Asset::<MarketId>::PoolShare(99);
        let combo = Asset::<MarketId>::CombinatorialToken([7u8; 32]);
        for asset in [pool, combo] {
            let encoded = asset.encode();
            assert_eq!(
                asset,
                Asset::<MarketId>::decode(&mut &encoded[..]).expect("asset roundtrip")
            );
        }
    }

    #[test]
    fn type_widths_match_nexus_contract() {
        let _market_id: MarketId = 0;
        let _pool_id: PoolId = 0;
        let _category: CategoryIndex = 0;
        let _combinatorial: CombinatorialId = [0u8; 32];
        assert_eq!(core::mem::size_of::<BlockNumber>(), 4);
        assert_eq!(core::mem::size_of::<MarketId>(), 16);
        assert_eq!(core::mem::size_of::<PoolId>(), 16);
    }
}
