// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

import {IDispatcher, DispatchPost, PostRequest, IncomingPostRequest} from
    "@polytope-labs/ismp-solidity-abi/contracts/IIsmp.sol";
import {PostRequestTimeout, IApp} from "@polytope-labs/ismp-solidity-abi/contracts/IApp.sol";

/// @title MockIsmpHost
/// @notice Test double for the ISMP host. Acts as `IDispatcher` for outbound
/// requests (records commitments, computes a deterministic `bytes32`), and as
/// the trusted caller for inbound `onAccept` / `onPostRequestTimeout` callbacks
/// via `deliver` / `deliverTimeout`. This lets unit + integration tests drive
/// the full burn/mint/refund lifecycle without a real Hyperbridge coprocessor.
///
/// ISMP host 的测试替身。对出站请求充当 `IDispatcher`（记录 commitment，确定性
/// 生成 `bytes32`）；对入站 `onAccept` / `onPostRequestTimeout` 通过 `deliver` /
/// `deliverTimeout` 作为可信调用方驱动。使单元与集成测试无需真实 Hyperbridge
/// 协处理器即可跑完整 burn/mint/refund 生命周期。
contract MockIsmpHost is IDispatcher {
    address public module; // the NEX token contract (set on first dispatch)

    uint256 public perByteFeeBps = 1; // flat per-byte fee for `perByteFee`
    uint256 public flatDispatchFee = 0; // returned by `dispatchFee`

    uint256 public dispatchCount;
    mapping(bytes32 => DispatchPost) public dispatched; // commitment => request

    event Dispatched(bytes32 indexed commitment, bytes dest, bytes to, uint256 amount);
    event Delivered(bytes32 indexed commitment, address indexed to, uint256 amount);
    event TimedOut(bytes32 indexed commitment, address indexed refundee, uint256 amount);

    /// @dev Allow tests to bind the NEX token contract that receives callbacks.
    function setModule(address module_) external {
        module = module_;
    }

    function setPerByteFeeBps(uint256 v) external {
        perByteFeeBps = v;
    }

    // ------------------------------------------------------------------- IDispatcher

    function dispatch(DispatchPost memory request) external payable returns (bytes32 commitment) {
        require(msg.sender == module || module == address(0), "MockHost: not module");
        if (module == address(0)) module = msg.sender;

        commitment = keccak256(abi.encodePacked(dispatchCount, msg.sender, request.dest, request.body));
        dispatched[commitment] = request;
        dispatchCount += 1;
        emit Dispatched(commitment, request.dest, request.to, msg.value);
    }

    function dispatchFee(DispatchPost memory request) external returns (uint256) {
        // Per-byte + base, matching the real host's two-component model.
        return flatDispatchFee + perByteFeeBps * request.body.length;
    }

    function perByteFee(bytes memory) external view returns (uint256) {
        return perByteFeeBps;
    }

    // ----------------------------------------------------------- inbound simulation

    /// @notice Simulate a verified inbound POST from `source`/`from` to the NEX
    /// module, carrying `body`. Mints to the recipient inside the token.
    /// 模拟来自 `source`/`from` 的已验证入站 POST，携带 `body`；在代币内向接收方铸造。
    function deliver(
        bytes memory source,
        bytes memory from,
        bytes memory body,
        address /*recipient*/
    ) external returns (bytes32 commitment) {
        PostRequest memory req = PostRequest({
            source: source,
            from: from,
            dest: bytes(""),
            to: bytes(""),
            body: body,
            timeout: 0,
            fee: 0,
            payer: address(0),
            timestamp: uint64(block.timestamp)
        });
        commitment = keccak256(abi.encodePacked("in", source, from, body));
        IncomingPostRequest memory incoming = IncomingPostRequest({request: req});
        IApp(module).onAccept(incoming);
        emit Delivered(commitment, address(0), 0);
    }

    /// @notice Simulate a timeout of a previously dispatched outbound request.
    /// Re-mints to the original sender inside the token.
    /// 模拟某笔已派发出站请求的超时；在代币内向原发送方重铸。
    function deliverTimeout(bytes32 commitment) external {
        DispatchPost memory post = dispatched[commitment];
        require(post.body.length > 0, "MockHost: unknown commitment");

        PostRequest memory req = PostRequest({
            source: bytes(""),
            from: bytes(""),
            dest: post.dest,
            to: post.to,
            body: post.body,
            timeout: post.timeout,
            fee: post.fee,
            payer: post.payer,
            timestamp: uint64(block.timestamp)
        });
        PostRequestTimeout memory timeout = PostRequestTimeout({request: req, relayer: msg.sender});
        IApp(module).onPostRequestTimeout(timeout);
        emit TimedOut(commitment, address(0), 0);
    }
}
