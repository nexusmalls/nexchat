// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

/// Minimal interface of the pinned WrappedHyperFungibleToken.
/// 锁定版本 WrappedHyperFungibleToken 的最小接口。
interface IWrappedHyperFungibleToken {
    struct WrappedConfigOptions {
        address host;
        address dispatcher;
        address underlying;
        bool isWeth;
    }

    function configure(WrappedConfigOptions calldata options) external;
    function addChain(bytes calldata chainId, bytes calldata moduleId) external;
    function removeChain(bytes calldata chainId) external;
    function pause() external;
    function unpause() external;
    function owner() external view returns (address);
    function host() external view returns (address);
    function dispatcher() external view returns (address);
    function underlying() external view returns (address);
    function isWeth() external view returns (bool);
}

/// Non-upgradeable, single-purpose owner for one Wrapped HFT instance.
/// 单个 Wrapped HFT 实例的不可升级、单一用途 owner。
///
/// It permanently separates one-time asset configuration, delayed peer
/// governance, and emergency pause authority. There is deliberately no generic
/// call, ownership transfer, upgrade, delegatecall, or asset sweep path.
///
/// 本合约永久分离一次性资产配置、延迟 peer 治理和紧急暂停权限。刻意不提供通用调用、
/// ownership 转移、升级、delegatecall 或资产提取路径。
contract HftGovernanceController {
    error Unauthorized();
    error AlreadyBound();
    error NotBound();
    error InvalidAddress();
    error InvalidConfiguration();
    error EmptyPeer();

    address public immutable timelock;
    address public immutable pauseGuardian;
    address public immutable configurator;

    IWrappedHyperFungibleToken public hft;
    bytes32 public lockedConfigurationHash;
    bool public configurationLocked;

    event HftBound(address indexed hft, bytes32 configHash);
    event ConfigurationPermanentlyLocked();
    event PeerAdded(bytes chainId, bytes moduleId);
    event PeerRemoved(bytes chainId);
    event PausedBy(address indexed guardian);
    event UnpausedBy(address indexed timelock);

    constructor(address timelock_, address pauseGuardian_, address configurator_) {
        if (
            timelock_ == address(0) || pauseGuardian_ == address(0)
                || configurator_ == address(0)
        ) {
            revert InvalidAddress();
        }
        timelock = timelock_;
        pauseGuardian = pauseGuardian_;
        configurator = configurator_;
    }

    modifier onlyTimelock() {
        if (msg.sender != timelock) revert Unauthorized();
        _;
    }

    modifier onlyGuardian() {
        if (msg.sender != pauseGuardian) revert Unauthorized();
        _;
    }

    modifier onlyConfigurator() {
        if (msg.sender != configurator) revert Unauthorized();
        _;
    }

    modifier whenBound() {
        if (!configurationLocked) revert NotBound();
        _;
    }

    /// Binds and configures the immutable HFT target exactly once.
    /// 仅一次绑定并配置不可变的 HFT target。
    function bindAndConfigure(
        address hft_,
        IWrappedHyperFungibleToken.WrappedConfigOptions calldata options
    ) external onlyConfigurator {
        if (configurationLocked || address(hft) != address(0)) revert AlreadyBound();
        if (
            hft_ == address(0) || options.host == address(0) || options.dispatcher == address(0)
                || options.underlying == address(0)
        ) {
            revert InvalidAddress();
        }
        // USDX V1 accepts ERC-20 USDC only; WETH/native mode is forbidden.
        // USDX V1 仅接受 ERC-20 USDC，禁止 WETH/原生币模式。
        if (options.isWeth) revert InvalidConfiguration();

        IWrappedHyperFungibleToken candidate = IWrappedHyperFungibleToken(hft_);
        if (candidate.owner() != address(this)) revert InvalidConfiguration();

        candidate.configure(options);
        if (
            candidate.host() != options.host || candidate.dispatcher() != options.dispatcher
                || candidate.underlying() != options.underlying
                || candidate.isWeth() != options.isWeth
        ) {
            revert InvalidConfiguration();
        }

        bytes32 configHash = keccak256(
            abi.encode(hft_, options.host, options.dispatcher, options.underlying, options.isWeth)
        );
        hft = candidate;
        lockedConfigurationHash = configHash;
        configurationLocked = true;

        emit HftBound(hft_, configHash);
        emit ConfigurationPermanentlyLocked();
    }

    /// Adds or replaces a peer through delayed governance.
    /// 通过延迟治理添加或替换 peer。
    function addChain(bytes calldata chainId, bytes calldata moduleId)
        external
        onlyTimelock
        whenBound
    {
        if (chainId.length == 0 || moduleId.length == 0) revert EmptyPeer();
        hft.addChain(chainId, moduleId);
        emit PeerAdded(chainId, moduleId);
    }

    /// Removes a peer through delayed governance.
    /// 通过延迟治理移除 peer。
    function removeChain(bytes calldata chainId) external onlyTimelock whenBound {
        if (chainId.length == 0) revert EmptyPeer();
        hft.removeChain(chainId);
        emit PeerRemoved(chainId);
    }

    /// Emergency guardian may only pause.
    /// 紧急 guardian 只能暂停。
    function pause() external onlyGuardian whenBound {
        hft.pause();
        emit PausedBy(msg.sender);
    }

    /// Only delayed governance may unpause.
    /// 仅延迟治理可解除暂停。
    function unpause() external onlyTimelock whenBound {
        hft.unpause();
        emit UnpausedBy(msg.sender);
    }
}
