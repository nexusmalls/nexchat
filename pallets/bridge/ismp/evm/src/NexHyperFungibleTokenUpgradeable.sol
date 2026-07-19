// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

// =============================================================================
// UUPS-upgradeable variant of NexHyperFungibleToken. Use this when the deployment
// must be upgradeable (e.g. behind a transparent/UUPS proxy on Polygon mainnet).
// The non-upgradeable `NexHyperFungibleToken.sol` is preferred for immutable
// deployments. Wire format is identical. NOT compiled in this Rust repo's CI;
// build with Foundry in the `evm/` workspace.
//
// NexHyperFungibleToken 的 UUPS 可升级变体。当部署必须可升级时（例如 Polygon 主网
// 透明/UUPS 代理后）使用。不可升级部署仍优先 `NexHyperFungibleToken.sol`。wire
// 格式完全一致。不纳入本 Rust 仓 CI；在 `evm/` 工作区用 Foundry 构建。
// =============================================================================

import {BaseIsmpModule} from "@polytope-labs/ismp-solidity-abi/contracts/IIsmpModule.sol";
import {
    IDispatcher,
    DispatchPost,
    PostRequest,
    IncomingPostRequest
} from "@polytope-labs/ismp-solidity-abi/contracts/IIsmp.sol";
import {PostRequestTimeout} from "@polytope-labs/ismp-solidity-abi/contracts/IApp.sol";

import {ERC20Upgradeable} from "@openzeppelin/contracts-upgradeable/token/ERC20/ERC20Upgradeable.sol";
import {IERC165} from "@openzeppelin/contracts/utils/introspection/IERC165.sol";
import {ERC165Upgradeable} from "@openzeppelin/contracts-upgradeable/utils/introspection/ERC165Upgradeable.sol";
import {OwnableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import {PausableUpgradeable} from "@openzeppelin/contracts-upgradeable/utils/PausableUpgradeable.sol";
import {UUPSUpgradeable} from "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import {Initializable} from "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";

import {IHyperFungibleToken} from "./interfaces/IHyperFungibleToken.sol";

/// @title NexHyperFungibleTokenUpgradeable
/// @notice UUPS-upgradeable burn/mint ERC-20 counterpart of `pallet-bridge-ismp`.
/// Logic is identical to `NexHyperFungibleToken`; only the upgradeability wiring
/// differs. The upgrade auth is `owner` (governance multi-sig in production).
///
/// `pallet-bridge-ismp` 的 UUPS 可升级 burn/mint ERC-20 对端。逻辑与
/// `NexHyperFungibleToken` 完全一致，仅可升级接线不同。升级权限归 `owner`
/// （生产环境为治理多签）。
contract NexHyperFungibleTokenUpgradeable is
    Initializable,
    BaseIsmpModule,
    ERC20Upgradeable,
    ERC165Upgradeable,
    OwnableUpgradeable,
    PausableUpgradeable,
    UUPSUpgradeable,
    IHyperFungibleToken
{
    address internal _host;
    address internal _dispatcher;
    mapping(bytes => bytes) internal _supportedChains;

    error InvalidAddress(uint256 length);
    error UnsupportedChain();
    error UnauthorizedSource();
    error ZeroAmount();
    error HostAlreadySet();

    /// @notice Replaces the constructor for proxy deployments. Call exactly once.
    /// @param host_  ISMP host contract address on this chain
    /// @param owner_ Initial owner (governance multi-sig in production)
    /// 替代构造器，用于代理部署。仅可调用一次。
    function initialize(address host_, address owner_) external initializer {
        __ERC20_init("Nexus", "NEX");
        __Ownable_init(owner_);
        __Pausable_init();
        __ERC165_init();
        __UUPSUpgradeable_init();
        _setIsmpHost(host_);
        _host = host_;
    }

    // ----------------------------------------------------------------- IHyperFungibleToken

    function host() public view override(BaseIsmpModule, IHyperFungibleToken) returns (address) {
        return _host;
    }

    function dispatcher() public view override returns (address) {
        return _dispatcher;
    }

    function supportedChain(bytes calldata chainId) public view override returns (bytes memory) {
        return _supportedChains[chainId];
    }

    function configure(ConfigOptions calldata options) external override onlyOwner {
        if (_host == address(0)) {
            _host = options.host;
        } else if (options.host != address(0) && options.host != _host) {
            revert HostAlreadySet();
        }
        _dispatcher = options.dispatcher;
    }

    function addChain(bytes calldata chainId, bytes calldata moduleId) external override onlyOwner {
        _supportedChains[chainId] = moduleId;
    }

    function removeChain(bytes calldata chainId) external override onlyOwner {
        delete _supportedChains[chainId];
    }

    function pause() external override onlyOwner {
        _pause();
    }

    function unpause() external override onlyOwner {
        _unpause();
    }

    function quote(SendParams calldata params) public override returns (uint256) {
        return quote(_buildDispatchPost(params));
    }

    function quote(DispatchPost memory request) public returns (uint256) {
        return IDispatcher(_host).perByteFee(request.dest) * request.body.length;
    }

    // --------------------------------------------------------------------- outbound

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

    function onAccept(IncomingPostRequest calldata incoming) external override onlyHost whenNotPaused {
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

    function onPostRequestTimeout(PostRequestTimeout memory incoming) external override onlyHost whenNotPaused {
        Message memory message = abi.decode(incoming.request.body, (Message));
        address refundee = _toAddr(message.from);
        _mint(refundee, message.amount);
        emit Refunded({to: refundee, amount: message.amount});
    }

    // ------------------------------------------------------------------- ERC20 pause

    function transfer(address to, uint256 value) public override whenNotPaused returns (bool) {
        return super.transfer(to, value);
    }

    function transferFrom(address from, address to, uint256 value) public override whenNotPaused returns (bool) {
        return super.transferFrom(from, to, value);
    }

    // ----------------------------------------------------------------- ERC165 / meta

    function supportsInterface(bytes4 interfaceId)
        public
        view
        override(ERC165Upgradeable, IERC165)
        returns (bool)
    {
        return interfaceId == type(IHyperFungibleToken).interfaceId || super.supportsInterface(interfaceId);
    }

    function decimals() public pure override returns (uint8) {
        return 18;
    }

    function _toAddr(bytes memory b) internal pure returns (address addr) {
        if (b.length != 20) revert InvalidAddress(b.length);
        // forge-lint: disable-next-line unsafe-typecast
        return address(bytes20(b));
    }
    /// @dev Upgrade auth restricted to the owner.
    function _authorizeUpgrade(address) internal view override onlyOwner {}
}

interface ICallDispatcher {
    function dispatch(bytes calldata data) external;
}
