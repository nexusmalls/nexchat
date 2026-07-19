// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

import {Script} from "forge-std/Script.sol";
import {WrappedHyperFungibleToken} from "@hyperbridge/core/contracts/apps/WrappedHyperFungibleToken.sol";
import {
    HftGovernanceController,
    IWrappedHyperFungibleToken
} from "../src/HftGovernanceController.sol";

/// Deploys and permanently configures the Polygon Amoy Wrapped HFT.
/// 部署并永久配置 Polygon Amoy Wrapped HFT。
contract DeployWrappedHftPolygon is Script {
    uint256 internal constant AMOY_CHAIN_ID = 80_002;
    address internal constant AMOY_USDC = 0x41E94Eb019C0762f9Bfcf9Fb1E58725BfB0e7582;

    function run()
        external
        returns (HftGovernanceController controller, WrappedHyperFungibleToken wrappedHft)
    {
        require(block.chainid == AMOY_CHAIN_ID, "Polygon Amoy only");

        uint256 deployerKey = vm.envUint("DEPLOYER_PRIVATE_KEY");
        address deployer = vm.addr(deployerKey);
        address timelock = vm.envAddress("TIMELOCK");
        address pauseGuardian = vm.envAddress("PAUSE_GUARDIAN");
        address host = vm.envAddress("AMOY_ISMP_HOST");
        address dispatcher = vm.envAddress("AMOY_CALL_DISPATCHER");

        vm.startBroadcast(deployerKey);
        controller = new HftGovernanceController(timelock, pauseGuardian, deployer);
        wrappedHft = new WrappedHyperFungibleToken(address(controller));
        controller.bindAndConfigure(
            address(wrappedHft),
            IWrappedHyperFungibleToken.WrappedConfigOptions({
                host: host,
                dispatcher: dispatcher,
                underlying: AMOY_USDC,
                isWeth: false
            })
        );
        vm.stopBroadcast();

        require(wrappedHft.owner() == address(controller), "controller is not owner");
        require(wrappedHft.host() == host, "host mismatch");
        require(wrappedHft.dispatcher() == dispatcher, "dispatcher mismatch");
        require(wrappedHft.underlying() == AMOY_USDC, "underlying mismatch");
        require(!wrappedHft.isWeth(), "WETH mode enabled");
        require(controller.configurationLocked(), "configuration not locked");
    }
}
