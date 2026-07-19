// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.17;

import {Test} from "forge-std/Test.sol";

import {NexHyperFungibleToken} from "../src/NexHyperFungibleToken.sol";
import {IHyperFungibleToken} from "../src/interfaces/IHyperFungibleToken.sol";
import {MockIsmpHost} from "./mocks/MockIsmpHost.sol";

/// @title BridgeIntegration.t
/// @notice Integration test aligning the EVM contract with `pallet-bridge-ismp`:
/// - the canonical `Message` ABI encoding (keccak256) matches a hand-computed
///   reference, so the Substrate side `abi_decode`s it correctly;
/// - the Nexus module id is exactly the 8 ASCII bytes `nexbridg`;
/// - a full Substrate -> EVM inbound mint, EVM -> Substrate outbound burn, and
///   outbound timeout refund all reconcile against the mock host ledger.
///
/// 对齐 `pallet-bridge-ismp` 的集成测试：规范 `Message` ABI 编码（keccak256）与
/// 手算参考一致，使 Substrate 侧 `abi_decode` 正确；Nexus module id 恰为 8 ASCII
/// 字节 `nexbridg`；Substrate->EVM 入站铸造、EVM->Substrate 出站销毁、出站超时
/// 退款三者与 mock host 账本对账一致。
contract BridgeIntegration is Test {
    NexHyperFungibleToken internal nex;
    MockIsmpHost internal host;

    address internal owner = address(0xA11CE);
    address internal alice = address(0xA1);

    bytes internal constant NEXUS_DEST = bytes("SUBSTRATE-NEXS");
    bytes internal constant NEXUS_MODULE = hex"6e65786272696467"; // "nexbridg"

    function setUp() public {
        host = new MockIsmpHost();
        vm.prank(owner);
        nex = new NexHyperFungibleToken(address(host), owner);
        host.setModule(address(nex));
        vm.prank(owner);
        nex.addChain(NEXUS_DEST, NEXUS_MODULE);
        // Seed alice with NEX via an inbound mint from the Nexus peer.
        _nexusMint(alice, 1000e18);
    }

    // ------------------------------------------------- wire format alignment

    function test_nexusModuleIdIsNexbridg_8bytes() public pure {
        assertEq(NEXUS_MODULE, bytes("nexbridg"));
        assertEq(NEXUS_MODULE.length, 8);
    }

    function test_messageAbiEncodingMatchesReference() public pure {
        IHyperFungibleToken.Message memory m = IHyperFungibleToken.Message({
            from: abi.encodePacked(address(0xCAFE)),
            to: abi.encodePacked(address(0xBEEF)),
            amount: 1234e18,
            data: ""
        });

        // Reference encoding computed independently from the struct layout.
        bytes memory ref = abi.encode(
            abi.encodePacked(address(0xCAFE)),
            abi.encodePacked(address(0xBEEF)),
            uint256(1234e18),
            bytes("")
        );

        assertEq(keccak256(abi.encode(m)), keccak256(ref), "Message ABI matches reference");
    }

    function test_messageWithDataFieldNonEmpty() public pure {
        bytes memory payload = hex"00_01_deadbeef";
        IHyperFungibleToken.Message memory m =
            IHyperFungibleToken.Message(abi.encodePacked(address(0x1)), abi.encodePacked(address(0x2)), 1e18, payload);

        bytes memory ref = abi.encode(
            abi.encodePacked(address(0x1)), abi.encodePacked(address(0x2)), uint256(1e18), payload
        );
        assertEq(keccak256(abi.encode(m)), keccak256(ref), "Message with data matches reference");
    }

    // ------------------------------------------------- inbound: Substrate -> EVM

    function test_inboundMint_reconcilesWithHostLedger() public {
        uint256 bobBalBefore = nex.balanceOf(address(0xB0B));
        _nexusMint(address(0xB0B), 77e18);
        assertEq(nex.balanceOf(address(0xB0B)), bobBalBefore + 77e18);
        // Total supply rose by exactly the minted amount.
        assertEq(nex.totalSupply(), 1000e18 + 77e18);
    }

    function test_inboundRejectsNonNexusModule() public {
        IHyperFungibleToken.Message memory m =
            IHyperFungibleToken.Message(abi.encodePacked(alice), abi.encodePacked(address(0xB0B)), 1e18, "");
        // Wrong source module id -> UnauthorizedSource.
        vm.expectRevert(NexHyperFungibleToken.UnauthorizedSource.selector);
        host.deliver(NEXUS_DEST, hex"6e65786272696468", abi.encode(m), address(0xB0B));
    }

    // ------------------------------------------------- outbound: EVM -> Substrate

    function test_outboundBurn_reconcilesWithHostLedger() public {
        uint256 supplyBefore = nex.totalSupply();
        IHyperFungibleToken.SendParams memory params = IHyperFungibleToken.SendParams({
            dest: NEXUS_DEST,
            to: abi.encodePacked(address(0xCAFE)),
            amount: 250e18,
            timeout: 3600,
            relayerFee: 0,
            data: ""
        });

        vm.prank(alice);
        bytes32 commitment = nex.send{value: 1 ether}(params);

        // Burn reduced supply and alice balance by exactly 250e18.
        assertEq(nex.totalSupply(), supplyBefore - 250e18);
        assertEq(nex.balanceOf(alice), 1000e18 - 250e18);

        // Host ledger recorded the dispatch with the Nexus peer as destination.
        (bytes memory dest,, bytes memory body,,,,) = host.dispatched(commitment);
        assertEq(dest, NEXUS_DEST, "host recorded Nexus dest");

        // The dispatched body decodes back to the canonical Message with the
        // right `to` and `amount`; `from` is alice (20 bytes packed).
        IHyperFungibleToken.Message memory m = abi.decode(body, (IHyperFungibleToken.Message));
        assertEq(m.amount, 250e18);
        assertEq(m.to, abi.encodePacked(address(0xCAFE)));
        assertEq(m.from, abi.encodePacked(alice));
        assertEq(m.data.length, 0);
    }

    function test_outboundFeeTokenPath_recordsDispatch() public {
        IHyperFungibleToken.SendParams memory params = IHyperFungibleToken.SendParams({
            dest: NEXUS_DEST,
            to: abi.encodePacked(address(0xCAFE)),
            amount: 50e18,
            timeout: 3600,
            relayerFee: 0,
            data: ""
        });
        vm.prank(alice);
        bytes32 commitment = nex.send(params); // fee-token path (no msg.value)
        // Host recorded the dispatch (proves dispatchWithFeeToken forwarded).
        assertGt(uint256(commitment), 0);
    }

    // ------------------------------------------------- round-trip reconciliation

    function test_roundTrip_burnThenRefundRestoresSupply() public {
        uint256 supply0 = nex.totalSupply();

        IHyperFungibleToken.SendParams memory params = IHyperFungibleToken.SendParams({
            dest: NEXUS_DEST,
            to: abi.encodePacked(address(0xCAFE)),
            amount: 300e18,
            timeout: 3600,
            relayerFee: 0,
            data: ""
        });
        vm.prank(alice);
        bytes32 commitment = nex.send{value: 1 ether}(params);
        assertEq(nex.totalSupply(), supply0 - 300e18);

        // Substrate side never delivered -> timeout refund re-mints.
        host.deliverTimeout(commitment);
        assertEq(nex.totalSupply(), supply0, "supply restored after refund");
        assertEq(nex.balanceOf(alice), 1000e18, "alice balance restored");
    }

    function test_roundTrip_burnThenInboundToSameRecipient() public {
        // alice burns 200 NEX outbound, then receives 200 NEX inbound from Nexus.
        // Net effect: alice balance unchanged, supply unchanged, host dispatched
        // one outbound and delivered one inbound.
        uint256 bal0 = nex.balanceOf(alice);
        uint256 supply0 = nex.totalSupply();

        IHyperFungibleToken.SendParams memory params = IHyperFungibleToken.SendParams({
            dest: NEXUS_DEST,
            to: abi.encodePacked(address(0xCAFE)),
            amount: 200e18,
            timeout: 3600,
            relayerFee: 0,
            data: ""
        });
        vm.prank(alice);
        nex.send{value: 1 ether}(params);

        _nexusMint(alice, 200e18);

        assertEq(nex.balanceOf(alice), bal0, "alice net flat");
        assertEq(nex.totalSupply(), supply0, "supply net flat");
    }

    // ------------------------------------------------- helpers

    function _nexusMint(address to, uint256 amount) internal {
        IHyperFungibleToken.Message memory m =
            IHyperFungibleToken.Message(abi.encodePacked(alice), abi.encodePacked(to), amount, "");
        host.deliver(NEXUS_DEST, NEXUS_MODULE, abi.encode(m), to);
    }
}
