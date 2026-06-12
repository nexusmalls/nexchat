use nexus_runtime::WASM_BINARY;
use sc_service::ChainType;
use sc_telemetry::serde_json::json;

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec;

fn chain_properties() -> sc_service::Properties {
    json!({
		"tokenSymbol": "NEX",
		"tokenDecimals": 12,
		"ss58Format": 273,
		"genesisInscription": concat!(
			"NEXUS GENESIS \u{2014} 2026-04-14 10:00\n",
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
		)
	})
	.as_object()
	.cloned()
	.unwrap()
}

pub fn development_chain_spec() -> Result<ChainSpec, String> {
    Ok(ChainSpec::builder(
        WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
        None,
    )
    .with_name("Nexus Development")
    .with_id("nexus_dev")
    .with_chain_type(ChainType::Development)
    .with_genesis_config_preset_name(sp_genesis_builder::DEV_RUNTIME_PRESET)
    .with_properties(chain_properties())
    .build())
}

pub fn local_chain_spec() -> Result<ChainSpec, String> {
    Ok(ChainSpec::builder(
        WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
        None,
    )
    .with_name("Nexus Local Testnet")
    .with_id("nexus_local")
    .with_chain_type(ChainType::Local)
    .with_genesis_config_preset_name(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET)
    .with_properties(chain_properties())
    .build())
}

pub fn mainnet_chain_spec() -> Result<ChainSpec, String> {
    Ok(ChainSpec::builder(
        WASM_BINARY.ok_or_else(|| "Wasm binary not available".to_string())?,
        None,
    )
    .with_name("Nexus")
    .with_id("nexus")
    .with_chain_type(ChainType::Live)
    .with_protocol_id("nexus")
    .with_genesis_config_preset_name(nexus_runtime::genesis_config_presets::MAINNET_RUNTIME_PRESET)
    .with_properties(chain_properties())
    // TODO: 上线前添加 bootnodes
    // .with_boot_nodes(vec![...])
    .build())
}

/// EN: Read `ss58Format` from chain spec properties and set the process-wide default so
/// JSON-RPC methods (e.g. `chat_*`) can decode SS58 `AccountId` parameters.
/// CN: 从 chain spec 的 `ss58Format` 设置进程级默认 SS58 前缀，供 `chat_*` 等 JSON-RPC 解析地址。
pub fn set_default_ss58_from_spec(chain_spec: &dyn sc_service::ChainSpec) {
    use sp_core::crypto::{set_default_ss58_version, Ss58AddressFormat};

    const NEXUS_SS58_PREFIX: u16 = 273;

    let ss58_format = chain_spec
        .properties()
        .get("ss58Format")
        .and_then(|v| v.as_u64())
        .map(|v| v as u16)
        .unwrap_or(NEXUS_SS58_PREFIX);

    set_default_ss58_version(Ss58AddressFormat::custom(ss58_format));
}
