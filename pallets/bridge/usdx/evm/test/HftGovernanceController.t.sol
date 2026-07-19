// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

import {
    HftGovernanceController,
    IWrappedHyperFungibleToken
} from "../src/HftGovernanceController.sol";

interface Vm {
    function prank(address) external;
    function expectRevert(bytes4) external;
}

contract MockWrappedHft is IWrappedHyperFungibleToken {
    address public immutable override owner;
    address public override host;
    address public override dispatcher;
    address public override underlying;
    bool public override isWeth;
    bool public paused;

    mapping(bytes32 => bytes) internal peers;

    constructor(address owner_) {
        owner = owner_;
    }

    modifier onlyOwner() {
        require(msg.sender == owner, "not owner");
        _;
    }

    function configure(WrappedConfigOptions calldata options) external override onlyOwner {
        if (host == address(0)) host = options.host;
        dispatcher = options.dispatcher;
        underlying = options.underlying;
        isWeth = options.isWeth;
    }

    function addChain(bytes calldata chainId, bytes calldata moduleId) external override onlyOwner {
        peers[keccak256(chainId)] = moduleId;
    }

    function removeChain(bytes calldata chainId) external override onlyOwner {
        delete peers[keccak256(chainId)];
    }

    function pause() external override onlyOwner {
        paused = true;
    }

    function unpause() external override onlyOwner {
        paused = false;
    }

    function peer(bytes calldata chainId) external view returns (bytes memory) {
        return peers[keccak256(chainId)];
    }
}

contract HftGovernanceControllerTest {
    Vm internal constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    address internal constant TIMELOCK = address(0x1001);
    address internal constant GUARDIAN = address(0x1002);
    address internal constant CONFIGURATOR = address(0x1003);
    address internal constant HOST = address(0x2001);
    address internal constant DISPATCHER = address(0x2002);
    address internal constant USDC = address(0x2003);

    HftGovernanceController internal controller;
    MockWrappedHft internal hft;

    function setUp() public {
        controller = new HftGovernanceController(TIMELOCK, GUARDIAN, CONFIGURATOR);
        hft = new MockWrappedHft(address(controller));
    }

    function options()
        internal
        pure
        returns (IWrappedHyperFungibleToken.WrappedConfigOptions memory)
    {
        return IWrappedHyperFungibleToken.WrappedConfigOptions({
            host: HOST,
            dispatcher: DISPATCHER,
            underlying: USDC,
            isWeth: false
        });
    }

    function bind() internal {
        vm.prank(CONFIGURATOR);
        controller.bindAndConfigure(address(hft), options());
    }

    function testBindLocksConfigurationExactlyOnce() public {
        bind();

        require(controller.configurationLocked(), "configuration not locked");
        require(address(controller.hft()) == address(hft), "wrong hft");
        require(hft.host() == HOST, "wrong host");
        require(hft.dispatcher() == DISPATCHER, "wrong dispatcher");
        require(hft.underlying() == USDC, "wrong underlying");
        require(!hft.isWeth(), "weth must be disabled");

        vm.prank(CONFIGURATOR);
        vm.expectRevert(HftGovernanceController.AlreadyBound.selector);
        controller.bindAndConfigure(address(hft), options());
    }

    function testRejectsWethConfiguration() public {
        IWrappedHyperFungibleToken.WrappedConfigOptions memory invalid = options();
        invalid.isWeth = true;
        vm.prank(CONFIGURATOR);
        vm.expectRevert(HftGovernanceController.InvalidConfiguration.selector);
        controller.bindAndConfigure(address(hft), invalid);
    }

    function testPeerManagementIsTimelockOnly() public {
        bind();
        bytes memory chainId = bytes("SUBSTRATE-NEXS");
        bytes memory moduleId = bytes("pall_hft");

        vm.prank(GUARDIAN);
        vm.expectRevert(HftGovernanceController.Unauthorized.selector);
        controller.addChain(chainId, moduleId);

        vm.prank(TIMELOCK);
        controller.addChain(chainId, moduleId);
        require(
            keccak256(hft.peer(chainId)) == keccak256(moduleId),
            "peer not installed"
        );
    }

    function testGuardianCanPauseButOnlyTimelockCanUnpause() public {
        bind();

        vm.prank(GUARDIAN);
        controller.pause();
        require(hft.paused(), "not paused");

        vm.prank(GUARDIAN);
        vm.expectRevert(HftGovernanceController.Unauthorized.selector);
        controller.unpause();

        vm.prank(TIMELOCK);
        controller.unpause();
        require(!hft.paused(), "not unpaused");
    }
}
