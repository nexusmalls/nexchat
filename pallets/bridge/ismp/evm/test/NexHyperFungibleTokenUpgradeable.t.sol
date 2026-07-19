// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

import {Test} from "forge-std/Test.sol";
import {Pausable} from "@openzeppelin/contracts/utils/Pausable.sol";
import {ERC1967Proxy} from
    "@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";

import {NexHyperFungibleTokenUpgradeable} from
    "../src/NexHyperFungibleTokenUpgradeable.sol";
import {IHyperFungibleToken} from "../src/interfaces/IHyperFungibleToken.sol";
import {MockIsmpHost} from "./mocks/MockIsmpHost.sol";

/// @title NexHyperFungibleTokenUpgradeableTest
/// @notice Verifies the UUPS variant: `initialize` once, upgrade auth restricted
/// to owner, and that the burn/mint/pause behaviour matches the non-upgradeable
/// contract. Mirrors `NexHyperFungibleTokenTest` for the key paths.
///
/// 校验 UUPS 变体：`initialize` 仅一次、升级权限限于 owner、burn/mint/pause 行为
/// 与不可升级合约一致。关键路径镜像 `NexHyperFungibleTokenTest`。
contract NexHyperFungibleTokenUpgradeableTest is Test {
    NexHyperFungibleTokenUpgradeable internal nex;
    NexHyperFungibleTokenUpgradeable internal impl;
    MockIsmpHost internal host;

    address internal owner = address(0xA11CE);
    address internal alice = address(0xA1);
    address internal bob = address(0xB0B);

    bytes internal constant NEXUS_DEST = bytes("SUBSTRATE-NEXS");
    bytes internal constant NEXUS_MODULE = hex"6e65786272696467";

    function setUp() public {
        host = new MockIsmpHost();
        impl = new NexHyperFungibleTokenUpgradeable();
        bytes memory data = abi.encodeWithSelector(
            NexHyperFungibleTokenUpgradeable.initialize.selector, address(host), owner
        );
        ERC1967Proxy proxy = new ERC1967Proxy(address(impl), data);
        nex = NexHyperFungibleTokenUpgradeable(address(proxy));

        host.setModule(address(nex));
        vm.prank(owner);
        nex.addChain(NEXUS_DEST, NEXUS_MODULE);

        IHyperFungibleToken.Message memory m =
            IHyperFungibleToken.Message(abi.encodePacked(alice), abi.encodePacked(alice), 1000e18, "");
        host.deliver(NEXUS_DEST, NEXUS_MODULE, abi.encode(m), alice);
    }

    function test_initialize_setsOwnerAndHost() public view {
        assertEq(nex.owner(), owner);
        assertEq(nex.host(), address(host));
        assertEq(nex.decimals(), 18);
    }

    function test_revert_initialize_twice() public {
        vm.expectRevert();
        nex.initialize(address(host), owner);
    }

    function test_supportsInterface_IHyperFungibleToken() public view {
        assertTrue(nex.supportsInterface(type(IHyperFungibleToken).interfaceId));
    }

    function test_send_burnsAndDispatches() public {
        uint256 balBefore = nex.balanceOf(alice);
        IHyperFungibleToken.SendParams memory params = IHyperFungibleToken.SendParams({
            dest: NEXUS_DEST,
            to: abi.encodePacked(address(0xCAFE)),
            amount: 100e18,
            timeout: 3600,
            relayerFee: 0,
            data: ""
        });
        vm.prank(alice);
        nex.send{value: 1 ether}(params);
        assertEq(nex.balanceOf(alice), balBefore - 100e18);
    }

    function test_timeout_reMints() public {
        IHyperFungibleToken.SendParams memory params = IHyperFungibleToken.SendParams({
            dest: NEXUS_DEST,
            to: abi.encodePacked(address(0xCAFE)),
            amount: 25e18,
            timeout: 3600,
            relayerFee: 0,
            data: ""
        });
        vm.prank(alice);
        bytes32 commitment = nex.send{value: 1 ether}(params);
        uint256 balBefore = nex.balanceOf(alice);
        host.deliverTimeout(commitment);
        assertEq(nex.balanceOf(alice), balBefore + 25e18);
    }

    function test_revert_pause_blocksTransfer() public {
        vm.prank(owner);
        nex.pause();
        vm.prank(alice);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        nex.transfer(bob, 1e18);
    }

    function test_revert_upgrade_notOwner() public {
        NexHyperFungibleTokenUpgradeable newImpl = new NexHyperFungibleTokenUpgradeable();
        vm.prank(alice);
        vm.expectRevert();
        nex.upgradeToAndCall(address(newImpl), "");
    }

    function test_upgrade_byOwner_succeeds() public {
        NexHyperFungibleTokenUpgradeable newImpl = new NexHyperFungibleTokenUpgradeable();
        vm.prank(owner);
        nex.upgradeToAndCall(address(newImpl), "");
        // State preserved across upgrade.
        assertGt(nex.balanceOf(alice), 0);
        assertEq(nex.owner(), owner);
    }
}
