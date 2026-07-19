// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

import {PostRequest, IncomingPostRequest} from "./IIsmp.sol";

struct PostRequestTimeout {
    PostRequest request;
    address relayer;
}

interface IApp {
    function onAccept(IncomingPostRequest calldata incoming) external;
    function onPostRequestTimeout(PostRequestTimeout memory incoming) external;
}
