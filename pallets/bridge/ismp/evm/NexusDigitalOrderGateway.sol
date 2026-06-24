// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

// =============================================================================
// REFERENCE CONTRACT — NOT compiled in this repo's CI and NOT a substitute for an
// independent audit. It shows the EVM counterpart of HB-ENT-01 (cross-chain digital
// ordering + derived-account withdraw) for `pallet-bridge-ismp`. The security-critical
// detail captured here is the **SCALE byte layout** of `InboundOp` carried in
// `Message.data`; see ./README.md ("Cross-order & withdraw payload").
//
// 参考合约 —— 不纳入本仓 CI，且不能替代独立审计。展示 `pallet-bridge-ismp` 的 HB-ENT-01
//（跨链数字下单 + 派生账户提款）EVM 对端。此处沉淀的安全关键细节是 `Message.data` 中
// `InboundOp` 的 **SCALE 字节布局**；见 ./README.md（“跨链下单与提款负载”）。
// =============================================================================

import {BaseIsmpModule} from "@polytope-labs/ismp-solidity-abi/contracts/IIsmpModule.sol";
import {IDispatcher, DispatchPost} from "@polytope-labs/ismp-solidity-abi/contracts/IIsmp.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";

/// @dev Minimal burn interface the NEX token must expose to this gateway. The token
/// (e.g. `NexHyperFungibleToken` / Polytope's `HyperFungibleToken`) must authorise the
/// gateway to burn caller balances. NEX 代币须对本网关暴露的最小销毁接口；代币须授权
/// 网关销毁调用者余额。
interface INexBurnable {
    function burnFrom(address from, uint256 amount) external;
}

/// @title NexusDigitalOrderGateway
/// @notice EVM entry point for HB-ENT-01. `placeDigitalOrder` burns NEX and POSTs a
/// SCALE `InboundOp::Order` to the Nexus bridge module; `withdraw` POSTs a SCALE
/// `InboundOp::Withdraw` to move the caller's derived-account NEX back to an EVM
/// recipient (authorised here by `msg.sender == owner`). The body is the canonical
/// `Message` (ABI-encoded), whose `data` is SCALE-encoded — matching
/// `pallets/bridge/ismp/src/types.rs`.
contract NexusDigitalOrderGateway is BaseIsmpModule, Ownable {
    /// @notice Canonical cross-chain message; byte-identical to the Rust `Message`.
    struct Message {
        bytes from; // original EVM sender (timeout refunds)
        bytes to; // unused by the order/withdraw paths (buyer comes from the payload)
        uint256 amount; // order: NEX burned (18 dp); withdraw: 0
        bytes data; // SCALE-encoded InboundOp
    }

    /// @notice The Nexus peer: `dest` = state-machine id bytes ("SUBSTRATE-NEXS"),
    /// `moduleId` = the 8 ASCII bytes "nexbridg".
    bytes public nexusDest;
    bytes public nexusModuleId;

    INexBurnable public immutable nex;
    bool public paused;

    event OrderPlaced(address indexed buyer, uint64 productId, uint256 amount, bytes32 commitment);
    event WithdrawRequested(address indexed owner, uint256 amount, address dest, bytes32 commitment);
    event PeerSet(bytes dest, bytes moduleId);
    event PausedSet(bool paused);

    error BridgePaused();
    error PeerNotSet();
    error ZeroAmount();

    constructor(address host_, address nex_, address owner_) Ownable(owner_) {
        _setIsmpHost(host_);
        nex = INexBurnable(nex_);
    }

    // ----------------------------------------------------------------- governance

    function setNexusPeer(bytes calldata dest, bytes calldata moduleId) external onlyOwner {
        nexusDest = dest;
        nexusModuleId = moduleId;
        emit PeerSet(dest, moduleId);
    }

    function setPaused(bool p) external onlyOwner {
        paused = p;
        emit PausedSet(p);
    }

    // -------------------------------------------------------------------- outbound

    /// @notice Burn `amount` NEX and place a cross-chain digital order on Nexus.
    /// @param productId   target product id
    /// @param quantity    order quantity
    /// @param amount      NEX to burn (18 dp); also the buyer's on-chain budget
    /// @param maxNex      slippage cap (18 dp); 0 = no extra cap beyond `amount`
    /// @param referrer    optional referrer EVM address (address(0) = none)
    /// @param timeout     request TTL in seconds (0 = never)
    /// @param relayerFee  fee offered to relayers (host fee token)
    function placeDigitalOrder(
        uint64 productId,
        uint32 quantity,
        uint256 amount,
        uint256 maxNex,
        address referrer,
        uint64 nonce,
        uint64 timeout,
        uint256 relayerFee
    ) external payable returns (bytes32 commitment) {
        if (paused) revert BridgePaused();
        if (amount == 0) revert ZeroAmount();
        if (nexusModuleId.length == 0) revert PeerNotSet();

        nex.burnFrom(msg.sender, amount);

        bytes memory order = abi.encodePacked(
            uint8(0), // InboundOp::Order variant index
            uint8(1), // schema_version
            bytes20(uint160(msg.sender)), // buyer_evm [u8;20]
            _le(productId, 8),
            _le(quantity, 4),
            _le(amount, 16),
            _le(maxNex, 16),
            _optAddr(referrer), // Option<[u8;20]>
            _le(nonce, 8)
        );

        commitment = _dispatch(abi.encodePacked(msg.sender), amount, order, timeout, relayerFee);
        emit OrderPlaced(msg.sender, productId, amount, commitment);
    }

    /// @notice Withdraw the caller's derived-account NEX from Nexus back to an EVM
    /// recipient. Authorisation: the derived account is `blake2_256("nexus-evm" ++
    /// msg.sender)`, so only the owning EVM key can drive it — enforced by binding
    /// `owner_evm = msg.sender`. The returned NEX is minted by the NEX token contract
    /// (a plain transfer), not by this gateway.
    function withdraw(uint256 amount, address dest, uint64 nonce, uint64 timeout, uint256 relayerFee)
        external
        payable
        returns (bytes32 commitment)
    {
        if (paused) revert BridgePaused();
        if (amount == 0) revert ZeroAmount();
        if (nexusModuleId.length == 0) revert PeerNotSet();

        bytes memory req = abi.encodePacked(
            uint8(1), // InboundOp::Withdraw variant index
            uint8(1), // schema_version
            bytes20(uint160(msg.sender)), // owner_evm [u8;20] (== caller)
            _le(amount, 16),
            bytes20(uint160(dest)), // dest_recipient [u8;20]
            _le(nonce, 8)
        );

        // Withdraw carries no inbound asset: Message.amount = 0.
        commitment = _dispatch(abi.encodePacked(msg.sender), 0, req, timeout, relayerFee);
        emit WithdrawRequested(msg.sender, amount, dest, commitment);
    }

    // --------------------------------------------------------------------- helpers

    function _dispatch(bytes memory from, uint256 amount, bytes memory data, uint64 timeout, uint256 fee)
        internal
        returns (bytes32)
    {
        Message memory message = Message({from: from, to: "", amount: amount, data: data});
        DispatchPost memory post = DispatchPost({
            dest: nexusDest,
            to: nexusModuleId,
            body: abi.encode(message),
            timeout: timeout,
            fee: fee,
            payer: msg.sender
        });
        return IDispatcher(host()).dispatch{value: msg.value}(post);
    }

    /// @dev SCALE little-endian encoding of `v` in `n` bytes (n <= 32).
    function _le(uint256 v, uint8 n) internal pure returns (bytes memory out) {
        out = new bytes(n);
        for (uint8 i = 0; i < n; i++) {
            out[i] = bytes1(uint8(v >> (8 * i)));
        }
    }

    /// @dev SCALE `Option<[u8;20]>`: `0x00` for none, else `0x01 ++ 20 bytes`.
    function _optAddr(address a) internal pure returns (bytes memory) {
        if (a == address(0)) return abi.encodePacked(uint8(0));
        return abi.encodePacked(uint8(1), bytes20(uint160(a)));
    }
}
