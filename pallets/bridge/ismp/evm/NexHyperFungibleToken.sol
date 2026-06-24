// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

// =============================================================================
// REFERENCE CONTRACT — NOT compiled in this repo's CI and NOT a substitute for
// an independent audit. For production, PREFER Polytope Labs' official, audited
// `HyperFungibleToken` (burn-custody / native mode), which encodes the IDENTICAL
// `Message` ABI used by `pallet-bridge-ismp`. This file exists so a team can
// deploy a standalone, self-custodied NEX token whose wire format byte-matches
// the Substrate side. See ./README.md for the deploy + `register_chain` mapping.
//
// 参考合约 —— 不纳入本仓 CI，且不能替代独立审计。生产环境**优先**使用 Polytope Labs
// 官方已审计的 `HyperFungibleToken`（burn 托管 / native 模式），它与
// `pallet-bridge-ismp` 使用**完全一致**的 `Message` ABI。此文件用于在需要自托管 NEX
// 代币时，给出与 Substrate 侧逐字节一致的参考实现。部署与 `register_chain` 映射见
// ./README.md。
// =============================================================================

import {BaseIsmpModule} from "@polytope-labs/ismp-solidity-abi/contracts/IIsmpModule.sol";
import {
    IDispatcher,
    DispatchPost,
    PostRequest,
    IncomingPostRequest
} from "@polytope-labs/ismp-solidity-abi/contracts/IIsmp.sol";

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";

/// @title NexHyperFungibleToken
/// @notice Burn/mint ERC-20 counterpart of `pallet-bridge-ismp`. Outbound: burns
/// the sender's NEX and dispatches an ISMP POST carrying the canonical `Message`.
/// Inbound: on a verified POST from the registered Nexus module, mints NEX to the
/// recipient. Timeouts re-mint to the original sender. Supply tracks cross-chain
/// flow, mirroring the native burn/mint on Nexus.
contract NexHyperFungibleToken is BaseIsmpModule, ERC20, Ownable {
    /// @notice The canonical cross-chain message. Byte-identical to the Rust
    /// `Message` vendored in `pallets/bridge/ismp/src/types.rs` and to Polytope
    /// Labs' `HyperFungibleToken.Message`.
    struct Message {
        bytes from; // original sender (for timeout refunds)
        bytes to; // recipient on the destination chain
        uint256 amount; // token amount, in THIS chain's ERC-20 precision
        bytes data; // optional calldata (ignored by the Stage-2 pallet)
    }

    /// @dev Per-peer config: `dest` is the destination state-machine id bytes
    /// (e.g. "SUBSTRATE-NEXS"), `moduleId` is the peer module id on that chain
    /// (Nexus pallet module id = the 8 ASCII bytes "nexbridg").
    struct Peer {
        bytes dest;
        bytes moduleId;
        bool enabled;
    }

    /// chainKey (keccak256(dest)) => Peer
    mapping(bytes32 => Peer) public peers;

    bool public paused;

    event Sent(address indexed from, bytes to, bytes dest, uint256 amount, bytes32 commitment);
    event Received(bytes from, address indexed to, bytes source, uint256 amount);
    event Refunded(address indexed to, bytes dest, uint256 amount);
    event PeerSet(bytes dest, bytes moduleId, bool enabled);
    event PausedSet(bool paused);

    error BridgePaused();
    error UnknownPeer();
    error ZeroAmount();
    error BadRecipient();

    constructor(address host_, address owner_) ERC20("Nexus", "NEX") Ownable(owner_) {
        // BaseIsmpModule resolves the ISMP host; some SDK versions take it via
        // constructor, others via an immutable setter. Adapt to the pinned SDK.
        _setIsmpHost(host_);
    }

    // ----------------------------------------------------------------- outbound

    /// @notice Burn `amount` NEX and dispatch a cross-chain transfer to `dest`.
    /// @param dest      destination state-machine id bytes (must be a registered peer)
    /// @param to        recipient bytes on the destination (32 bytes for Substrate)
    /// @param amount    NEX amount in this chain's ERC-20 precision (18 decimals)
    /// @param timeout   request TTL in seconds (0 = never)
    /// @param relayerFee fee offered to relayers, charged in the host fee token
    function send(bytes calldata dest, bytes calldata to, uint256 amount, uint64 timeout, uint256 relayerFee)
        external
        payable
        returns (bytes32 commitment)
    {
        if (paused) revert BridgePaused();
        if (amount == 0) revert ZeroAmount();
        if (to.length == 0) revert BadRecipient();

        Peer memory peer = peers[keccak256(dest)];
        if (!peer.enabled) revert UnknownPeer();

        // Real burn: total supply falls, mirroring the native burn on Nexus.
        _burn(msg.sender, amount);

        Message memory message = Message({
            from: abi.encodePacked(msg.sender),
            to: to,
            amount: amount,
            data: ""
        });

        DispatchPost memory post = DispatchPost({
            dest: peer.dest,
            to: peer.moduleId,
            body: abi.encode(message),
            timeout: timeout,
            fee: relayerFee,
            payer: msg.sender
        });

        commitment = IDispatcher(host()).dispatch{value: msg.value}(post);
        emit Sent(msg.sender, to, dest, amount, commitment);
    }

    // ------------------------------------------------------------------ inbound

    /// @inheritdoc BaseIsmpModule
    /// @dev Only the ISMP host may call this (post-proof verification + replay
    /// protection are enforced by the host before delivery).
    function onAccept(IncomingPostRequest calldata incoming) external override onlyHost {
        PostRequest calldata req = incoming.request;

        Peer memory peer = peers[keccak256(req.source)];
        // Authenticate the source module: must be our registered Nexus peer.
        require(peer.enabled, "NEX: unknown source chain");
        require(keccak256(req.from) == keccak256(peer.moduleId), "NEX: unknown source module");

        Message memory message = abi.decode(req.body, (Message));
        address recipient = _addressFromBytes(message.to);

        // Mint: total supply rises, mirroring the native mint on Nexus.
        _mint(recipient, message.amount);
        emit Received(message.from, recipient, req.source, message.amount);
    }

    /// @inheritdoc BaseIsmpModule
    /// @dev Outbound request timed out: refund (re-mint) the original sender.
    function onPostRequestTimeout(PostRequest calldata req) external override onlyHost {
        Message memory message = abi.decode(req.body, (Message));
        address sender = _addressFromBytes(message.from);
        _mint(sender, message.amount);
        emit Refunded(sender, req.dest, message.amount);
    }

    // ---------------------------------------------------------------- governance

    function setPeer(bytes calldata dest, bytes calldata moduleId, bool enabled) external onlyOwner {
        peers[keccak256(dest)] = Peer({dest: dest, moduleId: moduleId, enabled: enabled});
        emit PeerSet(dest, moduleId, enabled);
    }

    function setPaused(bool p) external onlyOwner {
        paused = p;
        emit PausedSet(p);
    }

    // -------------------------------------------------------------------- helpers

    function decimals() public pure override returns (uint8) {
        return 18;
    }

    /// @dev Decodes a 20-byte (EVM) or 32-byte (Substrate) recipient. For 32-byte
    /// Substrate accounts the low 20 bytes are NOT a valid EVM address; production
    /// deployments that bridge to EVM EOAs must agree on the 20-byte convention.
    function _addressFromBytes(bytes memory b) internal pure returns (address a) {
        require(b.length == 20 || b.length == 32, "NEX: bad address length");
        assembly {
            a := shr(96, mload(add(b, 32)))
        }
    }
}
