// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.18;

import "ds-test/test.sol";
import "../cheats/Vm.sol";
import {Globals} from "./Globals.sol";

contract ZkCheatcodesTest is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);

    uint256 constant ERA_FORK_BLOCK = 19579636;
    uint256 constant ERA_FORK_BLOCK_TS = 1700601590;

    uint256 constant ETH_FORK_BLOCK = 19225195;
    uint256 constant ETH_FORK_BLOCK_TS = 1707901427;

    address constant TEST_ADDRESS = 0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a;

    uint256 forkEra;
    uint256 forkEth;

    function setUp() public {
        forkEra = vm.createFork(Globals.ZKSYNC_MAINNET_URL, ERA_FORK_BLOCK);
        forkEth = vm.createFork(Globals.ETHEREUM_MAINNET_URL, ETH_FORK_BLOCK);
    }

    function testZkCheatcodesRoll() public {
        vm.selectFork(forkEra);
        require(block.number == ERA_FORK_BLOCK, "era block number mismatch");

        vm.roll(ERA_FORK_BLOCK + 1);
        require(block.number == ERA_FORK_BLOCK + 1, "era block number mismatch");
    }

    function testZkCheatcodesWarp() public {
        vm.selectFork(forkEra);
        require(block.timestamp == ERA_FORK_BLOCK_TS, "era block timestamp mismatch");

        vm.warp(ERA_FORK_BLOCK_TS + 1);
        require(block.timestamp == ERA_FORK_BLOCK_TS + 1, "era block timestamp mismatch");
    }

    function testZkCheatcodesDeal() public {
        vm.zkVm(true);
        require(TEST_ADDRESS.balance == 0, "era balance mismatch");

        vm.deal(TEST_ADDRESS, 100);
        require(TEST_ADDRESS.balance == 100, "era balance mismatch");
    }

    function testZkCheatcodesSetNonce() public {
        vm.zkVm(true);
        require(vm.getNonce(TEST_ADDRESS) == 0, "era nonce mismatch");

        vm.setNonce(TEST_ADDRESS, 10);
        require(vm.getNonce(TEST_ADDRESS) == 10, "era nonce mismatch");

        vm.resetNonce(TEST_ADDRESS);
        require(vm.getNonce(TEST_ADDRESS) == 0, "era nonce mismatch");
    }

    function testZkCheatcodesEtch() public {
        vm.zkVm(true);

        bytes32 emptyHash = hex"0000000000000000000000000000000000000000000000000000000000000000";
        bytes memory emptyBytes = hex"00";
        bytes32 zkBytecodeHash = hex"0100000f6d092b2cd44547a312320ad99c9587b40e0d03b0c17f09afd286d660";
        bytes memory zkDeployedBytecode =
            hex"0000008003000039000000400030043f0000000102200190000000120000c13d000000000201001900000009022001980000001a0000613d000000000101043b0000000a011001970000000b0110009c0000001a0000c13d0000000001000416000000000101004b0000001a0000c13d0000000a01000039000000800010043f0000000c010000410000001d0001042e0000000001000416000000000101004b0000001a0000c13d00000020010000390000010000100443000001200000044300000008010000410000001d0001042e00000000010000190000001e000104300000001c000004320000001d0001042e0000001e000104300000000000000000000000020000000000000000000000000000004000000100000000000000000000000000000000000000000000000000fffffffc000000000000000000000000ffffffff00000000000000000000000000000000000000000000000000000000643ceff9000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000020000000800000000000000000000000000000000000000000000000000000000000000000000000000000000075b6ac057b6098db0e2fae836aa00e54c6eec4973fc9e5e2b4c8baee23515b65";
        vm.zkRegisterContract("ConstantNumber", emptyHash, emptyBytes, emptyBytes, zkBytecodeHash, zkDeployedBytecode);

        vm.etch(TEST_ADDRESS, zkDeployedBytecode);

        (bool success, bytes memory output) = TEST_ADDRESS.call(abi.encodeWithSignature("ten()"));
        require(success, "ten() call failed");

        uint8 number = abi.decode(output, (uint8));
        require(number == 10, "era etched code incorrect");
    }
}
