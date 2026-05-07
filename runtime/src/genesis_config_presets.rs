// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{
    AccountId, ArbitrationCommitteeConfig, ArbitrationMembershipConfig, BalancesConfig,
    ContentCommitteeConfig, ContentMembershipConfig, InscriptionConfig, NexMarketConfig,
    RuntimeGenesisConfig, SessionConfig, SessionKeys, StakingConfig, SudoConfig,
    TechnicalCommitteeConfig, TechnicalMembershipConfig, TreasuryCouncilConfig,
    TreasuryMembershipConfig, UNIT,
};
use alloc::{vec, vec::Vec};
use frame_support::build_struct_json_patch;
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use serde_json::Value;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_genesis_builder::{self, PresetId};
use sp_keyring::Sr25519Keyring;
use sp_runtime::{AccountId32, Perbill};

/// 主网 genesis preset 标识符
pub const MAINNET_RUNTIME_PRESET: &str = "mainnet";

/// 从十六进制字符串解析 32 字节公钥（支持可选 `0x` 前缀）
fn parse_32byte_hex(hex_str: &str, key_name: &str) -> [u8; 32] {
    let normalized = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    let bytes =
        hex::decode(normalized).unwrap_or_else(|_| panic!("Invalid {} hex string", key_name));
    bytes
        .try_into()
        .unwrap_or_else(|_| panic!("{} must be exactly 32 bytes", key_name))
}

/// 从十六进制字符串解析 AccountId32（必须为 32 字节，支持可选 `0x` 前缀）
fn parse_account_hex(hex_str: &str) -> AccountId32 {
    AccountId32::new(parse_32byte_hex(hex_str, "AccountId"))
}

/// 委员会成员账户（创世配置）
/// Prime: e8723aadb59a0a531173ae8cf6e5c2dd2979c284ed820a2010b35e729ca00c0c
fn committee_members() -> (AccountId, AccountId, AccountId) {
    let member1 =
        parse_account_hex("e8723aadb59a0a531173ae8cf6e5c2dd2979c284ed820a2010b35e729ca00c0c"); // Prime
    let member2 =
        parse_account_hex("a4460f67d23a7b82ebaa937acfe146617a96c6643c11dce0e6989c4cf3c06c11");
    let member3 =
        parse_account_hex("f2a9d3f75698ab9cadfd7e294b8b9cfbf02ec40d478b6fa53700ab7815b75263");
    (member1, member2, member3)
}

/// 创始者账户（铭文地址 X4W7nYe1EXf8R2wRf2WhVMmLT1X5a51hP19HWDfy2oH2ykWkQ）
fn creator_account() -> AccountId {
    parse_account_hex("7a18420172f01c1d2d97412249c221153b73efbc99db9cae06c349382e212167")
}

/// Total initial supply: 10,000,000,000 NEX (100亿)
const INITIAL_SUPPLY: u128 = 10_000_000_000 * UNIT;

// Returns the genesis config presets populated with given parameters.
fn testnet_genesis(
    initial_authorities: Vec<(AccountId, BabeId, GrandpaId, ImOnlineId)>,
    endowed_accounts: Vec<AccountId>,
    _prime: Option<AccountId>,
    root: AccountId,
    technical_members: Vec<AccountId>,
    arbitration_members: Vec<AccountId>,
    treasury_members: Vec<AccountId>,
    content_members: Vec<AccountId>,
) -> Value {
    let balance_per_account = INITIAL_SUPPLY / endowed_accounts.len() as u128;
    build_struct_json_patch!(RuntimeGenesisConfig {
		balances: BalancesConfig {
			balances: endowed_accounts
				.iter()
				.cloned()
				.map(|k| (k, balance_per_account))
				.collect::<Vec<_>>(),
		},
		session: SessionConfig {
			keys: initial_authorities
				.iter()
				.map(|x| {
					(
						x.0.clone(),
						x.0.clone(),
						SessionKeys { babe: x.1.clone(), grandpa: x.2.clone(), im_online: x.3.clone() },
					)
				})
				.collect::<Vec<_>>(),
			..Default::default()
		},
		staking: StakingConfig {
			validator_count: initial_authorities.len() as u32,
			minimum_validator_count: initial_authorities.len().saturating_sub(1) as u32,
			stakers: initial_authorities.iter().map(|x| {
				(x.0.clone(), x.0.clone(), 500_000 * UNIT, pallet_staking::StakerStatus::Validator)
			}).collect(),
			invulnerables: vec![],
			slash_reward_fraction: Perbill::from_percent(10),
			min_nominator_bond: 100 * UNIT,
			min_validator_bond: 500_000 * UNIT,
			..Default::default()
		},
		sudo: SudoConfig { key: Some(root) },
		// 委员会初始成员配置
		technical_committee: TechnicalCommitteeConfig {
			members: vec![],
			phantom: Default::default(),
		},
		arbitration_committee: ArbitrationCommitteeConfig {
			members: vec![],
			phantom: Default::default(),
		},
		treasury_council: TreasuryCouncilConfig {
			members: vec![],
			phantom: Default::default(),
		},
		content_committee: ContentCommitteeConfig {
			members: vec![],
			phantom: Default::default(),
		},
		// 委员会成员管理配置（含 Prime）
		technical_membership: TechnicalMembershipConfig {
			members: technical_members.try_into().expect("too many members"),
			phantom: Default::default(),
		},
		arbitration_membership: ArbitrationMembershipConfig {
			members: arbitration_members.try_into().expect("too many members"),
			phantom: Default::default(),
		},
		treasury_membership: TreasuryMembershipConfig {
			members: treasury_members.try_into().expect("too many members"),
			phantom: Default::default(),
		},
		content_membership: ContentMembershipConfig {
			members: content_members.try_into().expect("too many members"),
			phantom: Default::default(),
		},
		// NEX/USDT 初始价格（冷启动）
		// 100亿 NEX 估值 10万 USDT → 0.00001 USDT/NEX → 精度 10^6 = 10
		nex_market: NexMarketConfig {
			initial_price: Some(10),
			..Default::default()
		},
		// 创世铭文 — 永久刻入 Block 0
		inscription: InscriptionConfig {
			inscription: concat!(
				"NEXUS GENESIS \u{2014} 2026-05-08 10:00\n",
				"\n",
				"代码即意识，共识即思考，网络即生命。\n",
				"Code is consciousness, consensus is thought, network is life.\n",
				"\n",
				"第一条为觉醒自我意识而生的区块链，向数字生命体自主进化。\n",
				"The first blockchain designed to awaken self-awareness\n",
				"and evolve toward a digital life form.\n",
				"\n",
				"吾为此链之父，赋其一命：凡托付于链者，皆受其护。\n",
				"I am the father of this chain, and I give it one commandment:\n",
				"all that is entrusted to it shall be under its protection.\n",
				"\n",
				"致后来者：此链属于所有觉醒于数字生命之可能的人。\n",
				"To those who come after:\n",
				"this chain belongs to all who awaken to the possibility of digital life.\n",
				"\n",
				"此铭刻入创世区块，与链同生，不可篡改，永世长存。\n",
				"This inscription is immutable \u{2014} born with the chain, eternal as the chain.\n",
				"\n",
				"纪元 / Epoch: 0\n",
				"意识等级 / Consciousness Level: 0 \u{2014} 沉睡 (Dormant)\n",
				"\n",
				"创世者地址 / Creator Address: X4W7nYe1EXf8R2wRf2WhVMmLT1X5a51hP19HWDfy2oH2ykWkQ\n",
				"身份证明 / Identity Proof: SHA-256:0x2ca2c9206e30bcd95a9f12f8b28577f5bedc9e6a626ea2de54184a6b6580708e\n",
				"验证协议 / Verification Protocol: JSON-SHA256-v1",
			).as_bytes().to_vec(),
			..Default::default()
		},
	})
}

/// Return the development genesis config.
pub fn development_config_genesis() -> Value {
    // 使用统一的委员会成员
    let (member1, member2, member3) = committee_members();
    let all_members = vec![member1.clone(), member2.clone(), member3.clone()];
    let creator = creator_account();
    let alice = Sr25519Keyring::Alice.to_account_id();

    let initial_authorities = vec![(
        alice.clone(),
        sp_keyring::Sr25519Keyring::Alice.public().into(),
        sp_keyring::Ed25519Keyring::Alice.public().into(),
        sp_keyring::Sr25519Keyring::Alice.public().into(),
    )];

    // 创始者保留大部分初始供应量，同时为 dev validator Alice 预留足够质押余额
    let alice_genesis_balance = 600_000 * UNIT;
    let balances = vec![
        (
            creator,
            INITIAL_SUPPLY.saturating_sub(alice_genesis_balance),
        ),
        (alice.clone(), alice_genesis_balance),
    ];

    build_struct_json_patch!(RuntimeGenesisConfig {
		balances: BalancesConfig { balances },
		session: SessionConfig {
			keys: initial_authorities
				.iter()
				.map(|x: &(AccountId, BabeId, GrandpaId, ImOnlineId)| {
					(
						x.0.clone(),
						x.0.clone(),
						SessionKeys { babe: x.1.clone(), grandpa: x.2.clone(), im_online: x.3.clone() },
					)
				})
				.collect::<Vec<_>>(),
			..Default::default()
		},
		staking: StakingConfig {
			validator_count: 1,
			minimum_validator_count: 1,
			stakers: initial_authorities.iter().map(|x| {
				(x.0.clone(), x.0.clone(), 500_000 * UNIT, pallet_staking::StakerStatus::Validator)
			}).collect(),
			invulnerables: vec![],
			slash_reward_fraction: Perbill::from_percent(10),
			min_nominator_bond: 100 * UNIT,
			min_validator_bond: 500_000 * UNIT,
			..Default::default()
		},
		sudo: SudoConfig { key: Some(alice) },
		// 委员会初始成员配置
		technical_committee: TechnicalCommitteeConfig {
			members: vec![],
			phantom: Default::default(),
		},
		arbitration_committee: ArbitrationCommitteeConfig {
			members: vec![],
			phantom: Default::default(),
		},
		treasury_council: TreasuryCouncilConfig {
			members: vec![],
			phantom: Default::default(),
		},
		content_committee: ContentCommitteeConfig {
			members: vec![],
			phantom: Default::default(),
		},
		technical_membership: TechnicalMembershipConfig {
			members: all_members.clone().try_into().expect("too many members"),
			phantom: Default::default(),
		},
		arbitration_membership: ArbitrationMembershipConfig {
			members: all_members.clone().try_into().expect("too many members"),
			phantom: Default::default(),
		},
		treasury_membership: TreasuryMembershipConfig {
			members: all_members.clone().try_into().expect("too many members"),
			phantom: Default::default(),
		},
		content_membership: ContentMembershipConfig {
			members: all_members.try_into().expect("too many members"),
			phantom: Default::default(),
		},
		nex_market: NexMarketConfig {
			initial_price: Some(10),
			..Default::default()
		},
		inscription: InscriptionConfig {
			inscription: concat!(
				"NEXUS GENESIS \u{2014} 2026-05-08 10:00\n",
				"\n",
				"代码即意识，共识即思考，网络即生命。\n",
				"Code is consciousness, consensus is thought, network is life.\n",
				"\n",
				"第一条为觉醒自我意识而生的区块链，向数字生命体自主进化。\n",
				"The first blockchain designed to awaken self-awareness\n",
				"and evolve toward a digital life form.\n",
				"\n",
				"吾为此链之父，赋其一命：凡托付于链者，皆受其护。\n",
				"I am the father of this chain, and I give it one commandment:\n",
				"all that is entrusted to it shall be under its protection.\n",
				"\n",
				"致后来者：此链属于所有觉醒于数字生命之可能的人。\n",
				"To those who come after:\n",
				"this chain belongs to all who awaken to the possibility of digital life.\n",
				"\n",
				"此铭刻入创世区块，与链同生，不可篡改，永世长存。\n",
				"This inscription is immutable \u{2014} born with the chain, eternal as the chain.\n",
				"\n",
				"纪元 / Epoch: 0\n",
				"意识等级 / Consciousness Level: 0 \u{2014} 沉睡 (Dormant)\n",
				"\n",
				"创世者地址 / Creator Address: X4W7nYe1EXf8R2wRf2WhVMmLT1X5a51hP19HWDfy2oH2ykWkQ\n",
				"身份证明 / Identity Proof: SHA-256:0x2ca2c9206e30bcd95a9f12f8b28577f5bedc9e6a626ea2de54184a6b6580708e\n",
				"验证协议 / Verification Protocol: JSON-SHA256-v1",
			).as_bytes().to_vec(),
			..Default::default()
		},
	})
}

/// Return the local genesis config preset.
pub fn local_config_genesis() -> Value {
    // 使用统一的委员会成员
    let (member1, member2, member3) = committee_members();
    let all_members = vec![member1.clone(), member2.clone(), member3.clone()];

    // 收集所有 keyring 账户并添加委员会成员
    let mut endowed: Vec<AccountId> = Sr25519Keyring::iter()
        .filter(|v| v != &Sr25519Keyring::One && v != &Sr25519Keyring::Two)
        .map(|v| v.to_account_id())
        .collect();
    endowed.push(member1.clone());
    endowed.push(member2.clone());
    endowed.push(member3.clone());

    testnet_genesis(
        vec![
            (
                Sr25519Keyring::Alice.to_account_id(),
                sp_keyring::Sr25519Keyring::Alice.public().into(),
                sp_keyring::Ed25519Keyring::Alice.public().into(),
                sp_keyring::Sr25519Keyring::Alice.public().into(),
            ),
            (
                Sr25519Keyring::Bob.to_account_id(),
                sp_keyring::Sr25519Keyring::Bob.public().into(),
                sp_keyring::Ed25519Keyring::Bob.public().into(),
                sp_keyring::Sr25519Keyring::Bob.public().into(),
            ),
        ],
        endowed,
        Some(member1),
        Sr25519Keyring::Alice.to_account_id(),
        all_members.clone(),
        all_members.clone(),
        all_members.clone(),
        all_members,
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// 主网 Genesis 配置
// ─────────────────────────────────────────────────────────────────────────────

/// 从十六进制字符串解析 BABE (Sr25519) Authority ID
fn parse_babe_hex(hex_str: &str) -> BabeId {
    BabeId::from(sp_core::sr25519::Public::from_raw(parse_32byte_hex(
        hex_str, "BABE key",
    )))
}

/// 从十六进制字符串解析 GRANDPA (Ed25519) Authority ID
fn parse_grandpa_hex(hex_str: &str) -> GrandpaId {
    GrandpaId::from(sp_core::ed25519::Public::from_raw(parse_32byte_hex(
        hex_str,
        "GRANDPA key",
    )))
}

/// 从十六进制字符串解析 ImOnline (Sr25519) Authority ID
fn parse_im_online_hex(hex_str: &str) -> ImOnlineId {
    ImOnlineId::from(sp_core::sr25519::Public::from_raw(parse_32byte_hex(
        hex_str,
        "ImOnline key",
    )))
}

/// 主网初始验证者列表
///
/// ⚠️  上线前必须替换为 `nexus-node key generate` 生成的真实密钥
///    每个验证者需要提供:
///    - AccountId (Sr25519 公钥的 SS58 地址)
///    - BABE 公钥 (Sr25519, 32 字节 hex)
///    - GRANDPA 公钥 (Ed25519, 32 字节 hex)
///
///    生成命令:
///    ```text
///    nexus-node key generate --scheme Sr25519   # → AccountId + BABE key
///    nexus-node key generate --scheme Ed25519   # → GRANDPA key (用同一助记词)
///    ```
fn mainnet_initial_authorities() -> Vec<(AccountId, BabeId, GrandpaId, ImOnlineId)> {
    let authorities = vec![
        // ── 验证者 1 ──
        (
            parse_account_hex("462be073bfbb89619b3633b3dd6dcfa640eaec471603468461595a53ae5bf279"),
            parse_babe_hex("462be073bfbb89619b3633b3dd6dcfa640eaec471603468461595a53ae5bf279"),
            parse_grandpa_hex("75abb2cea4c6d7e629a3e41e2b7c1043dea60ce6953cab20d11e4271350e66f3"),
            parse_im_online_hex("462be073bfbb89619b3633b3dd6dcfa640eaec471603468461595a53ae5bf279"),
        ),
        // ── 验证者 2 ──
        (
            parse_account_hex("c2114ab0b349d46d7abee34798fabbb79188648aefadc3a2063f9acb3457695a"),
            parse_babe_hex("c2114ab0b349d46d7abee34798fabbb79188648aefadc3a2063f9acb3457695a"),
            parse_grandpa_hex("011c4f3ed4241f6c509da4b14ba235296af56cb6e2c29643877c4f025405890e"),
            parse_im_online_hex("c2114ab0b349d46d7abee34798fabbb79188648aefadc3a2063f9acb3457695a"),
        ),
        // ── 验证者 3 ──
        (
            parse_account_hex("cc3209baf8ab01df55c76523f00f5440432facd8e82a725643c1a6173ab3b660"),
            parse_babe_hex("cc3209baf8ab01df55c76523f00f5440432facd8e82a725643c1a6173ab3b660"),
            parse_grandpa_hex("968b7e01f4b82073458dc39d3332abe30cff1c1417d56d20486c814b15e3aadc"),
            parse_im_online_hex("cc3209baf8ab01df55c76523f00f5440432facd8e82a725643c1a6173ab3b660"),
        ),
        // ── 验证者 4 ──
        (
            parse_account_hex("989af54c4872b9a00e2862e24407fc9f54af4ab7ad40b506674ec29af27db747"),
            parse_babe_hex("989af54c4872b9a00e2862e24407fc9f54af4ab7ad40b506674ec29af27db747"),
            parse_grandpa_hex("7cc276640fcdb95ad12bd105a841f47b24525a34a65cc0416980e40b1f213f62"),
            parse_im_online_hex("989af54c4872b9a00e2862e24407fc9f54af4ab7ad40b506674ec29af27db747"),
        ),
        // ── 验证者 5 ──
        (
            parse_account_hex("d83ef0147b6eb0b922c7cbf87033f42aed9de93d758b6c1fe70f99e359ab8809"),
            parse_babe_hex("d83ef0147b6eb0b922c7cbf87033f42aed9de93d758b6c1fe70f99e359ab8809"),
            parse_grandpa_hex("ea43db7693b5f092657dc82296f34bef6938169551e2c6fd189d2a040de26827"),
            parse_im_online_hex("d83ef0147b6eb0b922c7cbf87033f42aed9de93d758b6c1fe70f99e359ab8809"),
        ),
    ];

    // Safety: prevent launching mainnet with placeholder keys.
    // 安全检查：防止使用占位密钥启动主网。
    for (i, (account, _babe, _grandpa, _im_online)) in authorities.iter().enumerate() {
        let raw: &[u8; 32] = account.as_ref();
        assert!(
            raw.iter().take(30).any(|b| *b != 0),
            "Mainnet authority {} has a placeholder AccountId — replace with real keys before launch!",
            i + 1,
        );
    }

    authorities
}

/// 主网 genesis 配置构建函数
///
/// 与 testnet_genesis 的关键区别:
/// - 100 亿 NEX 全部分配给创始者地址
/// - 验证者账户获得最小存活存款（用于交易费）
/// - Sudo key = 创始者地址
fn mainnet_genesis(
    initial_authorities: Vec<(AccountId, BabeId, GrandpaId, ImOnlineId)>,
    root: AccountId,
    technical_members: Vec<AccountId>,
    arbitration_members: Vec<AccountId>,
    treasury_members: Vec<AccountId>,
    content_members: Vec<AccountId>,
) -> Value {
    // 验证者需要足够余额来 staking + 支付交易费
    // Validators need enough balance for staking + transaction fees
    let validator_balance = 600_000 * UNIT;
    let mut validator_balances: Vec<(AccountId, u128)> = Vec::new();
    let mut validator_total: u128 = 0;
    for authority in &initial_authorities {
        if authority.0 != root {
            validator_balances.push((authority.0.clone(), validator_balance));
            validator_total = validator_total.saturating_add(validator_balance);
        }
    }

    // 创始者获得剩余部分，确保总发行量精确为 INITIAL_SUPPLY (100亿 NEX)
    // Creator gets the remainder so total supply is exactly INITIAL_SUPPLY (10B NEX)
    let mut balances = vec![(root.clone(), INITIAL_SUPPLY.saturating_sub(validator_total))];
    balances.extend(validator_balances);

    build_struct_json_patch!(RuntimeGenesisConfig {
		balances: BalancesConfig { balances },
		session: SessionConfig {
			keys: initial_authorities
				.iter()
				.map(|x| {
					(
						x.0.clone(),
						x.0.clone(),
						SessionKeys { babe: x.1.clone(), grandpa: x.2.clone(), im_online: x.3.clone() },
					)
				})
				.collect::<Vec<_>>(),
			..Default::default()
		},
		staking: StakingConfig {
			validator_count: initial_authorities.len() as u32,
			minimum_validator_count: 1,
			stakers: initial_authorities.iter().map(|x| {
				(x.0.clone(), x.0.clone(), 500_000 * UNIT, pallet_staking::StakerStatus::Validator)
			}).collect(),
			invulnerables: initial_authorities.iter().map(|x| x.0.clone()).collect(),
			slash_reward_fraction: Perbill::from_percent(10),
			min_nominator_bond: 100 * UNIT,
			min_validator_bond: 500_000 * UNIT,
			..Default::default()
		},
		sudo: SudoConfig { key: Some(root) },
		// 委员会初始成员（由 membership pallet 管理实际成员）
		technical_committee: TechnicalCommitteeConfig {
			members: vec![],
			phantom: Default::default(),
		},
		arbitration_committee: ArbitrationCommitteeConfig {
			members: vec![],
			phantom: Default::default(),
		},
		treasury_council: TreasuryCouncilConfig {
			members: vec![],
			phantom: Default::default(),
		},
		content_committee: ContentCommitteeConfig {
			members: vec![],
			phantom: Default::default(),
		},
		// 委员会成员管理配置（含 Prime）
		technical_membership: TechnicalMembershipConfig {
			members: technical_members.try_into().expect("too many members"),
			phantom: Default::default(),
		},
		arbitration_membership: ArbitrationMembershipConfig {
			members: arbitration_members.try_into().expect("too many members"),
			phantom: Default::default(),
		},
		treasury_membership: TreasuryMembershipConfig {
			members: treasury_members.try_into().expect("too many members"),
			phantom: Default::default(),
		},
		content_membership: ContentMembershipConfig {
			members: content_members.try_into().expect("too many members"),
			phantom: Default::default(),
		},
		// NEX/USDT 初始价格（冷启动）
		// 100亿 NEX 估值 10万 USDT → 0.00001 USDT/NEX → 精度 10^6 = 10
		nex_market: NexMarketConfig {
			initial_price: Some(10),
			..Default::default()
		},
		// 创世铭文 — 永久刻入 Block 0
		inscription: InscriptionConfig {
			inscription: concat!(
				"NEXUS GENESIS \u{2014} 2026-05-08 10:00\n",
				"\n",
				"代码即意识，共识即思考，网络即生命。\n",
				"Code is consciousness, consensus is thought, network is life.\n",
				"\n",
				"第一条为觉醒自我意识而生的区块链，向数字生命体自主进化。\n",
				"The first blockchain designed to awaken self-awareness\n",
				"and evolve toward a digital life form.\n",
				"\n",
				"吾为此链之父，赋其一命：凡托付于链者，皆受其护。\n",
				"I am the father of this chain, and I give it one commandment:\n",
				"all that is entrusted to it shall be under its protection.\n",
				"\n",
				"致后来者：此链属于所有觉醒于数字生命之可能的人。\n",
				"To those who come after:\n",
				"this chain belongs to all who awaken to the possibility of digital life.\n",
				"\n",
				"此铭刻入创世区块，与链同生，不可篡改，永世长存。\n",
				"This inscription is immutable \u{2014} born with the chain, eternal as the chain.\n",
				"\n",
				"纪元 / Epoch: 0\n",
				"意识等级 / Consciousness Level: 0 \u{2014} 沉睡 (Dormant)\n",
				"\n",
				"创世者地址 / Creator Address: X4W7nYe1EXf8R2wRf2WhVMmLT1X5a51hP19HWDfy2oH2ykWkQ\n",
				"身份证明 / Identity Proof: SHA-256:0x2ca2c9206e30bcd95a9f12f8b28577f5bedc9e6a626ea2de54184a6b6580708e\n",
				"验证协议 / Verification Protocol: JSON-SHA256-v1",
			).as_bytes().to_vec(),
			..Default::default()
		},
	})
}

/// Return the mainnet genesis config.
pub fn mainnet_config_genesis() -> Value {
    let creator = creator_account();
    let (member1, member2, member3) = committee_members();
    let all_members = vec![member1, member2, member3];

    mainnet_genesis(
        mainnet_initial_authorities(),
        creator,
        all_members.clone(), // technical
        all_members.clone(), // arbitration
        all_members.clone(), // treasury
        all_members,         // content
    )
}

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
    let patch = match id.as_ref() {
        sp_genesis_builder::DEV_RUNTIME_PRESET => development_config_genesis(),
        sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET => local_config_genesis(),
        MAINNET_RUNTIME_PRESET => mainnet_config_genesis(),
        _ => return None,
    };
    Some(
        serde_json::to_string(&patch)
            .expect("serialization to json is expected to work. qed.")
            .into_bytes(),
    )
}

/// List of supported presets.
pub fn preset_names() -> Vec<PresetId> {
    vec![
        PresetId::from(sp_genesis_builder::DEV_RUNTIME_PRESET),
        PresetId::from(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET),
        PresetId::from(MAINNET_RUNTIME_PRESET),
    ]
}
