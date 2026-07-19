// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

import {IERC165} from "@openzeppelin/contracts/utils/introspection/IERC165.sol";

/// @title IHyperFungibleToken
/// @notice Cross-chain fungible token that burns on the source chain and mints on
/// the destination chain. Each deployment is its own bridge application — there is
/// no shared custody pool. Byte-identical to Polytope Labs' official interface so
/// the `@hyperbridge/sdk` can detect deployments via ERC165 (`0x7200c457`).
///
/// 跨链可替代代币：源链销毁、目的链铸造。每个部署本身即一个桥应用，无共享托管池。
/// 与 Polytope Labs 官方接口逐字节一致，`@hyperbridge/sdk` 可通过 ERC165
/// （`0x7200c457`）自动识别部署。
interface IHyperFungibleToken is IERC165 {
    /// @notice Parameters for initiating a cross-chain token transfer.
    /// 发起跨链转账的参数。
    struct SendParams {
        bytes dest; // destination state-machine id (e.g. bytes("EVM-137"))
        bytes to; // recipient on the destination chain (20 bytes for EVM)
        uint256 amount; // amount in THIS chain's ERC-20 precision
        uint64 timeout; // request TTL in seconds (0 = never)
        uint256 relayerFee; // fee offered to relayers (host fee token)
        bytes data; // optional calldata for the destination CallDispatcher
    }

    /// @notice ISMP host + CallDispatcher configuration.
    /// ISMP host 与 CallDispatcher 配置。
    struct ConfigOptions {
        address host; // ISMP host contract on this chain
        address dispatcher; // CallDispatcher for executing `data` on receive
    }

    /// @notice Canonical cross-chain message body. Byte-identical to the Rust
    /// `Message` in `pallets/bridge/ismp/src/types.rs` and to Polytope Labs'
    /// `HyperFungibleToken.Message`.
    /// 规范跨链消息体。与 `pallets/bridge/ismp/src/types.rs` 的 Rust `Message`
    /// 及 Polytope Labs `HyperFungibleToken.Message` 逐字节一致。
    struct Message {
        bytes from; // original sender (for timeout refunds)
        bytes to; // recipient on the destination chain
        uint256 amount; // token amount, in the DESTINATION chain's precision
        bytes data; // optional calldata (ignored by the Stage-2 pallet)
    }

    function host() external view returns (address);
    function dispatcher() external view returns (address);
    function supportedChain(bytes calldata chainId) external view returns (bytes memory);
    function configure(ConfigOptions calldata options) external;
    function addChain(bytes calldata chainId, bytes calldata moduleId) external;
    function removeChain(bytes calldata chainId) external;
    function pause() external;
    function unpause() external;
    function quote(SendParams calldata params) external returns (uint256);
    function send(SendParams calldata params) external payable returns (bytes32);

    event Sent(address from, bytes to, string dest, uint256 amount, bytes32 commitment);
    event Received(bytes from, address to, string source, uint256 amount);
    event Refunded(address to, uint256 amount);
}
