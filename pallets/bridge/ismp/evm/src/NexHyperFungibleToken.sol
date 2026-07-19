// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

// =============================================================================
// Upgraded reference contract for the EVM side of `pallet-bridge-ismp`.
//
// This is a drop-in replacement for Polytope Labs' official `HyperFungibleToken`
// (burn/mint, ERC-6160) that is byte-compatible with the Substrate side. It is
// still a REFERENCE implementation: production mainnet deployments should prefer
// the official, audited contract unless they need the self-custodied NEX flow
// captured here. NOT compiled in this Rust repo's CI; build with Foundry in the
// `evm/` workspace. See ./README.md for deploy + `register_chain` mapping.
//
// `pallet-bridge-ismp` EVM 侧的升级版参考合约。是 Polytope Labs 官方
// `HyperFungibleToken`（burn/mint、ERC-6160）的逐字节兼容替换件。仍为参考实现：
// 生产主网部署应优先使用官方已审计合约，除非需要此处沉淀的自托管 NEX 流程。
// 不纳入本 Rust 仓 CI；在 `evm/` 工作区用 Foundry 构建。部署与 `register_chain`
// 映射见 ./README.md。
// =============================================================================

import {BaseIsmpModule} from "@polytope-labs/ismp-solidity-abi/contracts/IIsmpModule.sol";
import {
    IDispatcher,
    DispatchPost,
    PostRequest,
    IncomingPostRequest
} from "@polytope-labs/ismp-solidity-abi/contracts/IIsmp.sol";
import {PostRequestTimeout} from "@polytope-labs/ismp-solidity-abi/contracts/IApp.sol";

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {Pausable} from "@openzeppelin/contracts/utils/Pausable.sol";
import {ERC165} from "@openzeppelin/contracts/utils/introspection/ERC165.sol";

import {IHyperFungibleToken} from "./interfaces/IHyperFungibleToken.sol";

/// @title NexHyperFungibleToken
/// @notice Burn/mint ERC-20 counterpart of `pallet-bridge-ismp`. Outbound: burns
/// the sender's NEX and dispatches an ISMP POST carrying the canonical `Message`.
/// Inbound: on a verified POST from the registered Nexus module, mints NEX to the
/// recipient. Timeouts re-mint to the original sender. Supply tracks cross-chain
/// flow, mirroring the native burn/mint on Nexus.
///
/// `pallet-bridge-ismp` 的 burn/mint ERC-20 对端。出站：销毁调用者 NEX 并派发携带
/// 规范 `Message` 的 ISMP POST。入站：在来自已注册 Nexus 模块的已验证 POST 上向
/// 接收方铸造 NEX。超时向原发送方重铸。供给随跨链流动，与 Nexus 原生 burn/mint 对应。
contract NexHyperFungibleToken is BaseIsmpModule, ERC20, ERC165, Ownable, Pausable, IHyperFungibleToken {
    /// @notice Address of the ISMP host contract on this chain. Set once.
    /// 本链 ISMP host 合约地址。只可设置一次。
    address internal _host;

    /// @notice Address of the CallDispatcher for executing `data` on receive.
    /// 用于在接收时执行 `data` 的 CallDispatcher 地址。
    address internal _dispatcher;

    /// @notice chainId (raw bytes) => peer module id on that chain.
    /// An empty value means the chain is not supported.
    /// chainId（原始 bytes）=> 该链上的对端 module id。空值表示未支持。
    mapping(bytes => bytes) internal _supportedChains;

    error InvalidAddress(uint256 length);
    error UnsupportedChain();
    error UnauthorizedSource();
    error ZeroAmount();
    error HostAlreadySet();

    constructor(address host_, address owner_) ERC20("Nexus", "NEX") Ownable(owner_) {
        // BaseIsmpModule resolves the ISMP host; some SDK versions take it via
        // constructor, others via an immutable setter. Adapt to the pinned SDK.
        _setIsmpHost(host_);
        _host = host_;
    }

    // ----------------------------------------------------------------- IHyperFungibleToken

    /// @inheritdoc IHyperFungibleToken
    function host() public view override(BaseIsmpModule, IHyperFungibleToken) returns (address) {
        return _host;
    }

    /// @inheritdoc IHyperFungibleToken
    function dispatcher() public view override returns (address) {
        return _dispatcher;
    }

    /// @inheritdoc IHyperFungibleToken
    function supportedChain(bytes calldata chainId) public view override returns (bytes memory) {
        return _supportedChains[chainId];
    }

    /// @inheritdoc IHyperFungibleToken
    function configure(ConfigOptions calldata options) external override onlyOwner {
        if (_host == address(0)) {
            _host = options.host;
        } else if (options.host != address(0) && options.host != _host) {
            revert HostAlreadySet();
        }
        _dispatcher = options.dispatcher;
    }

    /// @inheritdoc IHyperFungibleToken
    function addChain(bytes calldata chainId, bytes calldata moduleId) external override onlyOwner {
        _supportedChains[chainId] = moduleId;
    }

    /// @inheritdoc IHyperFungibleToken
    function removeChain(bytes calldata chainId) external override onlyOwner {
        delete _supportedChains[chainId];
    }

    /// @inheritdoc IHyperFungibleToken
    function pause() external override onlyOwner {
        _pause();
    }

    /// @inheritdoc IHyperFungibleToken
    function unpause() external override onlyOwner {
        _unpause();
    }

    /// @inheritdoc IHyperFungibleToken
    function quote(SendParams calldata params) public override returns (uint256) {
        return quote(_buildDispatchPost(params));
    }

    /// @notice Quote the fee in the host's fee token for a raw DispatchPost.
    /// Computes `perByteFee(dest) * body.length`, matching the Hyperbridge
    /// per-byte protocol fee model. Relayer tip is added by the caller.
    /// 对原始 DispatchPost 估算 host fee token 费用。按 Hyperbridge 按字节协议费
    /// 模型 `perByteFee(dest) * body.length` 计算。relayer 小费由调用方叠加。
    function quote(DispatchPost memory request) public returns (uint256) {
        return IDispatcher(_host).perByteFee(request.dest) * request.body.length;
    }

    // --------------------------------------------------------------------- outbound

    /// @inheritdoc IHyperFungibleToken
    function send(SendParams calldata params) external payable override whenNotPaused returns (bytes32) {
        if (params.amount == 0) revert ZeroAmount();
        _burn(msg.sender, params.amount);

        DispatchPost memory request = _buildDispatchPost(params);

        bytes32 commitment;
        if (msg.value > 0) {
            commitment = IDispatcher(_host).dispatch{value: msg.value}(request);
        } else {
            commitment = dispatchWithFeeToken(request);
        }

        emit Sent({
            from: msg.sender,
            to: params.to,
            dest: string(params.dest),
            amount: params.amount,
            commitment: commitment
        });
    }

    /// @dev Builds the DispatchPost from SendParams.
    function _buildDispatchPost(SendParams calldata params) internal view returns (DispatchPost memory) {
        bytes memory dest = _supportedChains[params.dest];
        if (dest.length == 0) revert UnsupportedChain();

        bytes memory body = abi.encode(
            Message({from: abi.encodePacked(msg.sender), to: params.to, amount: params.amount, data: params.data})
        );

        return DispatchPost({
            dest: params.dest,
            to: dest,
            body: body,
            timeout: params.timeout,
            fee: params.relayerFee,
            payer: msg.sender
        });
    }

    // ---------------------------------------------------------------------- inbound

    /// @notice Handles incoming cross-chain token transfer messages.
    /// @dev Called by the ISMP host after proof verification + replay protection.
    /// Verifies the source address matches the configured peer, then mints to the
    /// recipient. If `data` is present, executes it via the CallDispatcher.
    /// 处理入站跨链转账消息。由 ISMP host 在证明验证与重放保护后调用。校验来源地址
    /// 匹配已注册对端后向接收方铸造。若 `data` 非空，经 CallDispatcher 执行。
    function onAccept(IncomingPostRequest calldata incoming)
        external
        override
        onlyHost
        whenNotPaused
    {
        PostRequest calldata request = incoming.request;

        bytes memory expectedSource = _supportedChains[request.source];
        if (expectedSource.length == 0) revert UnsupportedChain();
        if (keccak256(request.from) != keccak256(expectedSource)) revert UnauthorizedSource();

        Message memory message = abi.decode(request.body, (Message));
        address beneficiary = _toAddr(message.to);
        _mint(beneficiary, message.amount);

        if (message.data.length > 0) {
            // forge-lint: disable-next-line unsafe-cast
            ICallDispatcher(_dispatcher).dispatch(message.data);
        }

        emit Received({
            from: message.from,
            to: beneficiary,
            source: string(request.source),
            amount: message.amount
        });
    }

    /// @notice Handles timeout of a previously dispatched cross-chain transfer.
    /// @dev Re-mints the burned tokens back to the original sender as a refund.
    /// 处理已派发跨链转账的超时。向原发送方重铸销毁的代币作为退款。
    function onPostRequestTimeout(PostRequestTimeout memory incoming)
        external
        override
        onlyHost
        whenNotPaused
    {
        Message memory message = abi.decode(incoming.request.body, (Message));
        address refundee = _toAddr(message.from);
        _mint(refundee, message.amount);
        emit Refunded({to: refundee, amount: message.amount});
    }

    // ------------------------------------------------------------------- ERC20 pause

    /// @notice Pauses ERC20 transfers.
    function transfer(address to, uint256 value) public override whenNotPaused returns (bool) {
        return super.transfer(to, value);
    }

    /// @notice Pauses ERC20 transferFrom.
    function transferFrom(address from, address to, uint256 value) public override whenNotPaused returns (bool) {
        return super.transferFrom(from, to, value);
    }

    // ----------------------------------------------------------------- ERC165 / meta

    /// @notice ERC165 interface detection. Returns true for `IHyperFungibleToken`
    /// (`0x7200c457`) and `IERC165`.
    function supportsInterface(bytes4 interfaceId)
        public
        view
        override(ERC165, IERC165)
        returns (bool)
    {
        return interfaceId == type(IHyperFungibleToken).interfaceId || super.supportsInterface(interfaceId);
    }

    function decimals() public pure override returns (uint8) {
        return 18;
    }

    /// @dev Extracts an address from a strictly 20-byte payload. Substrate
    /// 32-byte recipients are NOT valid EVM addresses; production deployments
    /// bridging to EVM EOAs must agree on the 20-byte convention.
    /// 从严格 20 字节负载解析地址。Substrate 32 字节接收人并非合法 EVM 地址；
    /// 桥接到 EVM EOA 的生产部署须约定 20 字节规范。
    function _toAddr(bytes memory b) internal pure returns (address addr) {
        if (b.length != 20) revert InvalidAddress(b.length);
        // forge-lint: disable-next-line unsafe-typecast
        return address(bytes20(b));
    }
}

/// @dev Minimal interface for the CallDispatcher that executes optional `data`
/// on the destination chain. Matches Polytope Labs' `ICallDispatcher`.
interface ICallDispatcher {
    function dispatch(bytes calldata data) external;
}
