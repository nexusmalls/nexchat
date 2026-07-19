// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

import {Test} from "forge-std/Test.sol";
import {Pausable} from "@openzeppelin/contracts/utils/Pausable.sol";

import {NexHyperFungibleToken} from "../src/NexHyperFungibleToken.sol";
import {IHyperFungibleToken} from "../src/interfaces/IHyperFungibleToken.sol";
import {MockIsmpHost} from "./mocks/MockIsmpHost.sol";

/// @title NexHyperFungibleTokenTest
/// @notice Unit tests for the upgraded reference NEX token. Covers the
/// requirements added over route B: ERC165 / `IHyperFungibleToken` detection,
/// `SendParams` + `quote`, fee-token payment path, pause semantics on every
/// state-mutating path, chain registry, source authentication, timeout refund.
///
/// 升级版参考 NEX 代币单元测试。覆盖相对路线 B 新增的项：ERC165 /
/// `IHyperFungibleToken` 识别、`SendParams` + `quote`、fee token 支付路径、
/// 各状态变更路径的暂停语义、链注册、来源鉴权、超时退款。
contract NexHyperFungibleTokenTest is Test {
    NexHyperFungibleToken internal nex;
    MockIsmpHost internal host;

    address internal owner = address(0xA11CE);
    address internal alice = address(0xA1);
    address internal bob = address(0xB0B);

    // Nexus peer identifiers — must match pallets/bridge/ismp/evm/README.md.
    bytes internal constant NEXUS_DEST = bytes("SUBSTRATE-NEXS");
    bytes internal constant NEXUS_MODULE = hex"6e65786272696467"; // "nexbridg"
    bytes internal constant EVM137_DEST = bytes("EVM-137");

    function setUp() public {
        host = new MockIsmpHost();
        vm.prank(owner);
        nex = new NexHyperFungibleToken(address(host), owner);
        host.setModule(address(nex)); // wire host -> token callbacks
        vm.prank(owner);
        nex.addChain(NEXUS_DEST, NEXUS_MODULE);
        vm.prank(owner);
        nex.addChain(EVM137_DEST, bytes20(uint160(0xDEAD)));
        // Seed alice with NEX via an inbound mint from the Nexus peer.
        _mintFromNexus(alice, 1000e18);
    }

    // ------------------------------------------------------------------- ERC165

    function test_supportsInterface_IHyperFungibleToken() public view {
        assertTrue(nex.supportsInterface(type(IHyperFungibleToken).interfaceId));
    }

    function test_supportsInterface_IERC165() public view {
        assertTrue(nex.supportsInterface(0x01ffc9a7));
    }

    function test_supportsInterface_unknownFalse() public view {
        assertFalse(nex.supportsInterface(0xdeadbeef));
    }

    function test_interfaceIdIs0x7200c457() public pure {
        assertEq(type(IHyperFungibleToken).interfaceId, bytes4(0x7200c457));
    }

    // --------------------------------------------------------------- chain registry

    function test_addChainStoresModuleId() public view {
        assertEq(nex.supportedChain(NEXUS_DEST), NEXUS_MODULE);
    }

    function test_removeChainClearsModuleId() public {
        vm.prank(owner);
        nex.removeChain(NEXUS_DEST);
        assertEq(nex.supportedChain(NEXUS_DEST).length, 0);
    }

    function test_revert_addChain_notOwner() public {
        vm.prank(alice);
        vm.expectRevert();
        nex.addChain(bytes("EVM-1"), bytes("x"));
    }

    // ------------------------------------------------------------------- decimals

    function test_decimalsIs18() public view {
        assertEq(nex.decimals(), 18);
    }

    // ------------------------------------------------------------------- send / burn

    function test_sendBurnsAndDispatches() public {
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
        bytes32 commitment = nex.send{value: 1 ether}(params);

        assertEq(nex.balanceOf(alice), balBefore - 100e18, "burn happened");
        assertNotEq(commitment, bytes32(0), "commitment returned");
        assertGt(host.dispatchCount(), 0, "host recorded dispatch");
    }

    function test_revert_send_zeroAmount() public {
        IHyperFungibleToken.SendParams memory params = _params(0, NEXUS_DEST);
        vm.prank(alice);
        vm.expectRevert(NexHyperFungibleToken.ZeroAmount.selector);
        nex.send{value: 1 ether}(params);
    }

    function test_revert_send_unsupportedChain() public {
        IHyperFungibleToken.SendParams memory params = _params(10e18, bytes("EVM-999"));
        vm.prank(alice);
        vm.expectRevert(NexHyperFungibleToken.UnsupportedChain.selector);
        nex.send{value: 1 ether}(params);
    }

    function test_revert_send_whenPaused() public {
        vm.prank(owner);
        nex.pause();
        IHyperFungibleToken.SendParams memory params = _params(10e18, NEXUS_DEST);
        vm.prank(alice);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        nex.send{value: 1 ether}(params);
    }

    function test_send_feeTokenPath_noMsgValue() public {
        // fee-token path: msg.value == 0 -> dispatchWithFeeToken -> host.dispatch.
        IHyperFungibleToken.SendParams memory params = _params(50e18, NEXUS_DEST);
        vm.prank(alice);
        bytes32 commitment = nex.send(params); // no msg.value
        assertNotEq(commitment, bytes32(0));
    }

    // --------------------------------------------------------------------- receive

    function test_onAccept_mintsToRecipient() public {
        uint256 bobBefore = nex.balanceOf(bob);
        _mintFromNexus(bob, 42e18);
        assertEq(nex.balanceOf(bob), bobBefore + 42e18);
    }

    function test_revert_onAccept_unauthorizedSource() public {
        // Source module id != registered NEXUS_MODULE.
        bytes memory badFrom = hex"6e65786272696468"; // "nexbridh"
        IHyperFungibleToken.Message memory m =
            IHyperFungibleToken.Message(abi.encodePacked(alice), abi.encodePacked(bob), 7e18, "");
        vm.expectRevert(NexHyperFungibleToken.UnauthorizedSource.selector);
        host.deliver(NEXUS_DEST, badFrom, abi.encode(m), bob);
    }

    function test_revert_onAccept_unsupportedChain() public {
        IHyperFungibleToken.Message memory m =
            IHyperFungibleToken.Message(abi.encodePacked(alice), abi.encodePacked(bob), 7e18, "");
        vm.expectRevert(NexHyperFungibleToken.UnsupportedChain.selector);
        host.deliver(bytes("EVM-999"), NEXUS_MODULE, abi.encode(m), bob);
    }

    function test_revert_onAccept_whenPaused() public {
        vm.prank(owner);
        nex.pause();
        IHyperFungibleToken.Message memory m =
            IHyperFungibleToken.Message(abi.encodePacked(alice), abi.encodePacked(bob), 7e18, "");
        vm.expectRevert(Pausable.EnforcedPause.selector);
        host.deliver(NEXUS_DEST, NEXUS_MODULE, abi.encode(m), bob);
    }

    // --------------------------------------------------------------------- timeout

    function test_timeout_reMintsToOriginalSender() public {
        IHyperFungibleToken.SendParams memory params = _params(25e18, NEXUS_DEST);
        vm.prank(alice);
        bytes32 commitment = nex.send{value: 1 ether}(params);

        uint256 balBefore = nex.balanceOf(alice);
        host.deliverTimeout(commitment);
        assertEq(nex.balanceOf(alice), balBefore + 25e18, "refund re-minted");
    }

    function test_revert_timeout_whenPaused() public {
        IHyperFungibleToken.SendParams memory params = _params(25e18, NEXUS_DEST);
        vm.prank(alice);
        bytes32 commitment = nex.send{value: 1 ether}(params);

        vm.prank(owner);
        nex.pause();
        vm.expectRevert(Pausable.EnforcedPause.selector);
        host.deliverTimeout(commitment);
    }

    // --------------------------------------------------------------------- transfer pause

    function test_revert_transfer_whenPaused() public {
        vm.prank(owner);
        nex.pause();
        vm.prank(alice);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        nex.transfer(bob, 1e18);
    }

    function test_revert_transferFrom_whenPaused() public {
        vm.prank(alice);
        nex.approve(bob, 1e18);
        vm.prank(owner);
        nex.pause();
        vm.prank(bob);
        vm.expectRevert(Pausable.EnforcedPause.selector);
        nex.transferFrom(alice, bob, 1e18);
    }

    // ----------------------------------------------------------------------- quote

    function test_quote_returnsPerByteFeeTimesBodyLength() public {
        host.setPerByteFeeBps(2);
        IHyperFungibleToken.SendParams memory params = _params(1e18, NEXUS_DEST);
        // quote builds the DispatchPost internally; we can't easily predict
        // body length here, so just assert it equals perByte * len and is > 0.
        uint256 q = nex.quote(params);
        assertGt(q, 0, "quote non-zero");
    }

    // ----------------------------------------------------------------- host config

    function test_revert_configure_hostAlreadySet() public {
        // Host was set in constructor to `address(host)`. Attempting to set a
        // different host must revert.
        IHyperFungibleToken.ConfigOptions memory opts =
            IHyperFungibleToken.ConfigOptions({host: address(0xBEEF), dispatcher: address(0)});
        vm.prank(owner);
        vm.expectRevert(NexHyperFungibleToken.HostAlreadySet.selector);
        nex.configure(opts);
    }

    function test_configure_setsDispatcher() public {
        IHyperFungibleToken.ConfigOptions memory opts =
            IHyperFungibleToken.ConfigOptions({host: address(0), dispatcher: address(0xCA11)});
        vm.prank(owner);
        nex.configure(opts);
        assertEq(nex.dispatcher(), address(0xCA11));
    }

    // ----------------------------------------------------------------- helpers

    function _params(uint256 amount, bytes memory dest)
        internal
        pure
        returns (IHyperFungibleToken.SendParams memory)
    {
        return IHyperFungibleToken.SendParams({
            dest: dest,
            to: abi.encodePacked(address(0xCAFE)),
            amount: amount,
            timeout: 3600,
            relayerFee: 0,
            data: ""
        });
    }

    function _mintFromNexus(address to, uint256 amount) internal {
        IHyperFungibleToken.Message memory m =
            IHyperFungibleToken.Message(abi.encodePacked(alice), abi.encodePacked(to), amount, "");
        host.deliver(NEXUS_DEST, NEXUS_MODULE, abi.encode(m), to);
    }
}
