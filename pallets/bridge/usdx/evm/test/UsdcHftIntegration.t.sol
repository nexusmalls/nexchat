// Copyright (C) Nexus contributors
// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

import {Test} from "forge-std/Test.sol";
import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {WrappedHyperFungibleToken} from "@hyperbridge/core/contracts/apps/WrappedHyperFungibleToken.sol";
import {HyperFungibleToken} from "@hyperbridge/core/contracts/apps/HyperFungibleToken.sol";
import {DispatchPost} from "@hyperbridge/core/contracts/interfaces/IDispatcher.sol";
import {
    IApp,
    IncomingPostRequest,
    PostRequestTimeout
} from "@hyperbridge/core/contracts/interfaces/IApp.sol";
import {PostRequest} from "@hyperbridge/core/contracts/libraries/Message.sol";
import {
    HftGovernanceController,
    IWrappedHyperFungibleToken
} from "../src/HftGovernanceController.sol";

contract MockUsdc is ERC20 {
    constructor() ERC20("Mock USDC", "USDC") {}

    function mint(address account, uint256 amount) external {
        _mint(account, amount);
    }

    function decimals() public pure override returns (uint8) {
        return 6;
    }
}

/// Minimal dispatcher and callback driver for the pinned official HFT ABI.
/// 锁定官方 HFT ABI 的最小 dispatcher 与 callback 驱动器。
contract MockHftHost {
    address public immutable feeToken;
    uint64 public nonce;
    DispatchPost internal lastRequest;

    constructor(address feeToken_) {
        feeToken = feeToken_;
    }

    function dispatch(DispatchPost memory request) external payable returns (bytes32 commitment) {
        lastRequest = request;
        nonce += 1;
        commitment = keccak256(abi.encode(nonce, msg.sender, request));
    }

    function deliver(
        address module,
        bytes memory source,
        bytes memory from,
        bytes memory body
    ) external {
        IApp(module).onAccept(
            IncomingPostRequest({
                request: PostRequest({
                    source: source,
                    dest: bytes("EVM-80002"),
                    nonce: nonce + 1,
                    from: from,
                    to: abi.encodePacked(module),
                    timeoutTimestamp: uint64(block.timestamp + 1 hours),
                    body: body
                }),
                relayer: msg.sender
            })
        );
    }

    function deliverLastTimeout(address module) external {
        IApp(module).onPostRequestTimeout(
            PostRequestTimeout({
                request: PostRequest({
                    source: bytes("EVM-80002"),
                    dest: lastRequest.dest,
                    nonce: nonce,
                    from: abi.encodePacked(module),
                    to: lastRequest.to,
                    timeoutTimestamp: uint64(block.timestamp),
                    body: lastRequest.body
                }),
                relayer: msg.sender
            })
        );
    }
}

/// Exercises USDC custody, HFT delivery, timeout and pause behavior.
/// 验证 USDC 托管、HFT 交付、timeout 与暂停行为。
contract UsdcHftIntegrationTest is Test {
    address internal constant TIMELOCK = address(0x1001);
    address internal constant GUARDIAN = address(0x1002);
    address internal constant CONFIGURATOR = address(0x1003);
    address internal constant USER = address(0x1004);
    address internal constant RECIPIENT = address(0x1005);
    bytes internal constant NEXUS_STATE_MACHINE = bytes("SUBSTRATE-NEXS-TEST-ONLY");
    bytes internal constant HFT_MODULE_ID = bytes("pall_hft");

    MockUsdc internal usdc;
    MockHftHost internal host;
    HftGovernanceController internal controller;
    WrappedHyperFungibleToken internal wrappedHft;

    function setUp() public {
        usdc = new MockUsdc();
        host = new MockHftHost(address(usdc));
        controller = new HftGovernanceController(TIMELOCK, GUARDIAN, CONFIGURATOR);
        wrappedHft = new WrappedHyperFungibleToken(address(controller));

        vm.prank(CONFIGURATOR);
        controller.bindAndConfigure(
            address(wrappedHft),
            IWrappedHyperFungibleToken.WrappedConfigOptions({
                host: address(host),
                dispatcher: address(0x2001),
                underlying: address(usdc),
                isWeth: false
            })
        );
        vm.prank(TIMELOCK);
        controller.addChain(NEXUS_STATE_MACHINE, HFT_MODULE_ID);

        usdc.mint(USER, 1_000_000);
        vm.prank(USER);
        usdc.approve(address(wrappedHft), type(uint256).max);
    }

    function sendParams(uint256 amount)
        internal
        pure
        returns (HyperFungibleToken.SendParams memory)
    {
        return HyperFungibleToken.SendParams({
            dest: NEXUS_STATE_MACHINE,
            to: abi.encodePacked(bytes32(uint256(0x1234))),
            amount: amount,
            timeout: 1 hours,
            relayerFee: 0,
            data: bytes("")
        });
    }

    function testSendLocksUsdcAndTimeoutRefundsSender() public {
        vm.prank(USER);
        wrappedHft.send(sendParams(400_000));
        assertEq(usdc.balanceOf(USER), 600_000);
        assertEq(usdc.balanceOf(address(wrappedHft)), 400_000);

        host.deliverLastTimeout(address(wrappedHft));
        assertEq(usdc.balanceOf(USER), 1_000_000);
        assertEq(usdc.balanceOf(address(wrappedHft)), 0);
    }

    function testAuthenticatedInboundDeliveryReleasesUsdc() public {
        usdc.mint(address(wrappedHft), 250_000);
        bytes memory body = abi.encode(
            HyperFungibleToken.Message({
                from: abi.encodePacked(USER),
                to: abi.encodePacked(RECIPIENT),
                amount: 250_000,
                data: bytes("")
            })
        );

        host.deliver(address(wrappedHft), NEXUS_STATE_MACHINE, HFT_MODULE_ID, body);
        assertEq(usdc.balanceOf(RECIPIENT), 250_000);
        assertEq(usdc.balanceOf(address(wrappedHft)), 0);
    }

    function testPauseCurrentlyBlocksTimeoutRefund() public {
        vm.prank(USER);
        wrappedHft.send(sendParams(400_000));
        vm.prank(GUARDIAN);
        controller.pause();

        vm.expectRevert();
        host.deliverLastTimeout(address(wrappedHft));
        assertEq(usdc.balanceOf(address(wrappedHft)), 400_000);
    }
}
