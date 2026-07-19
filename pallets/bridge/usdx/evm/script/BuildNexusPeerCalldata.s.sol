// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {HftGovernanceController} from "../src/HftGovernanceController.sol";

/// Builds controller calldata for scheduling through the configured timelock.
/// 构造经已配置 timelock 调度的 controller calldata。
contract BuildNexusPeerCalldata is Script {
    bytes internal constant HFT_MODULE_ID = bytes("pall_hft");

    function run() external returns (address target, bytes memory data) {
        target = vm.envAddress("HFT_CONTROLLER");
        bytes memory nexusStateMachine = vm.envBytes("NEXUS_STATE_MACHINE");
        require(nexusStateMachine.length != 0, "empty Nexus state machine");

        data = abi.encodeCall(
            HftGovernanceController.addChain,
            (nexusStateMachine, HFT_MODULE_ID)
        );
        console2.log("timelock target", target);
        console2.log("timelock calldata");
        console2.logBytes(data);
    }
}
