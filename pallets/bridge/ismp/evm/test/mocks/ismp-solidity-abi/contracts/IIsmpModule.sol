// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

import {DispatchPost, IDispatcher, IncomingPostRequest} from "./IIsmp.sol";
import {IApp, PostRequestTimeout} from "./IApp.sol";

/// @dev Abstract base for ISMP modules. Stores the host address set via
/// `_setIsmpHost` and exposes `onlyHost` + `dispatchWithFeeToken` helpers.
/// Declares `onAccept` / `onPostRequestTimeout` as virtual so concrete modules
/// override them. Mirrors the shape of Polytope Labs' `BaseIsmpModule` /
/// `HyperApp` for the methods the contracts under test rely on.
///
/// ISMP 模块抽象基类。存储经 `_setIsmpHost` 设置的 host 地址，暴露 `onlyHost` 与
/// `dispatchWithFeeToken` 辅助。将 `onAccept` / `onPostRequestTimeout` 声明为
/// virtual 供具体模块 override。镜像 Polytope Labs `BaseIsmpModule` / `HyperApp`
/// 中被测合约依赖的方法形状。
abstract contract BaseIsmpModule is IApp {
    address internal _ismpHost;

    modifier onlyHost() {
        require(msg.sender == _ismpHost, "BaseIsmpModule: only host");
        _;
    }

    function _setIsmpHost(address host_) internal {
        _ismpHost = host_;
    }

    function host() public view virtual returns (address) {
        return _ismpHost;
    }

    /// @dev In production this pulls the host fee token from `msg.sender` and
    /// dispatches. For tests we forward to the host's `dispatch` so the
    /// fee-token path is observable.
    function dispatchWithFeeToken(DispatchPost memory request) internal returns (bytes32) {
        return IDispatcher(_ismpHost).dispatch(request);
    }

    /// @dev Default no-op; concrete modules override.
    function onAccept(IncomingPostRequest calldata) external virtual override {}

    /// @dev Default no-op; concrete modules override.
    function onPostRequestTimeout(PostRequestTimeout memory) external virtual override {}
}
