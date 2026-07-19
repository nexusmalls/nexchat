// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

import {Test} from "forge-std/Test.sol";
import {WrappedHyperFungibleToken} from "@hyperbridge/core/contracts/apps/WrappedHyperFungibleToken.sol";
import {
    HftGovernanceController,
    IWrappedHyperFungibleToken
} from "../src/HftGovernanceController.sol";

/// Verifies controller invariants against the pinned official Wrapped HFT.
/// 使用锁定的官方 Wrapped HFT 验证 controller 不变量。
contract WrappedHftConfigurationTest is Test {
    address internal constant TIMELOCK = address(0x1001);
    address internal constant GUARDIAN = address(0x1002);
    address internal constant CONFIGURATOR = address(0x1003);
    address internal constant HOST = address(0x2001);
    address internal constant DISPATCHER = address(0x2002);
    address internal constant USDC = address(0x2003);

    HftGovernanceController internal controller;
    WrappedHyperFungibleToken internal wrappedHft;

    function setUp() public {
        controller = new HftGovernanceController(TIMELOCK, GUARDIAN, CONFIGURATOR);
        wrappedHft = new WrappedHyperFungibleToken(address(controller));

        vm.prank(CONFIGURATOR);
        controller.bindAndConfigure(
            address(wrappedHft),
            IWrappedHyperFungibleToken.WrappedConfigOptions({
                host: HOST,
                dispatcher: DISPATCHER,
                underlying: USDC,
                isWeth: false
            })
        );
    }

    function testOfficialWrappedHftIsPermanentlyOwnedAndConfiguredByController() public view {
        assertEq(wrappedHft.owner(), address(controller));
        assertEq(wrappedHft.host(), HOST);
        assertEq(wrappedHft.dispatcher(), DISPATCHER);
        assertEq(wrappedHft.underlying(), USDC);
        assertFalse(wrappedHft.isWeth());
        assertTrue(controller.configurationLocked());
    }

    function testOnlyTimelockCanInstallCanonicalHftPeer() public {
        bytes memory nexusStateMachine = hex"01020304";
        bytes memory moduleId = bytes("pall_hft");

        vm.prank(GUARDIAN);
        vm.expectRevert(HftGovernanceController.Unauthorized.selector);
        controller.addChain(nexusStateMachine, moduleId);

        vm.prank(TIMELOCK);
        controller.addChain(nexusStateMachine, moduleId);
        assertEq(wrappedHft.supportedChain(nexusStateMachine), moduleId);
    }

    function testExternalAccountsCannotReconfigureOfficialWrappedHft() public {
        WrappedHyperFungibleToken.WrappedConfigOptions memory replacement =
            WrappedHyperFungibleToken.WrappedConfigOptions({
                host: HOST,
                dispatcher: address(0x3001),
                underlying: address(0x3002),
                isWeth: false
            });

        vm.expectRevert();
        wrappedHft.configure(replacement);
        assertEq(wrappedHft.dispatcher(), DISPATCHER);
        assertEq(wrappedHft.underlying(), USDC);
    }

    function testControllerExposesNoOwnershipTransferPath() public {
        (bool ok,) = address(controller).call(
            abi.encodeWithSignature("transferOwnership(address)", address(this))
        );
        assertFalse(ok);
        assertEq(wrappedHft.owner(), address(controller));
    }
}
