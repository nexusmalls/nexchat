// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

// Request types + dispatcher interface. Structs live here so a single import
// gives consumers everything they need.
// 请求类型 + 派发器接口。结构体集中于此，单一 import 即可获取全部。

struct PostRequest {
    bytes source; // source state-machine id bytes
    bytes from; // source module id
    bytes dest; // destination state-machine id bytes
    bytes to; // destination module id
    bytes body; // abi-encoded Message
    uint64 timeout; // seconds
    uint256 fee; // relayer fee in fee token
    address payer; // refund payer
    uint64 timestamp; // dispatch timestamp (set by host)
}

struct IncomingPostRequest {
    PostRequest request;
}

struct DispatchPost {
    bytes dest;
    bytes to;
    bytes body;
    uint64 timeout;
    uint256 fee;
    address payer;
}

interface IDispatcher {
    function dispatch(DispatchPost memory request) external payable returns (bytes32);
    function dispatchFee(DispatchPost memory request) external returns (uint256);
    function perByteFee(bytes memory dest) external view returns (uint256);
}
