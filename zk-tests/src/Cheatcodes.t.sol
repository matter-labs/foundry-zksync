// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test} from "forge-std/Test.sol";

contract ZkCheatcodesTest is Test {
    uint256 constant ERA_FORK_BLOCK = 19579636;
    uint256 constant ERA_FORK_BLOCK_TS = 1700601590;

    uint256 constant ETH_FORK_BLOCK = 19225195;
    uint256 constant ETH_FORK_BLOCK_TS = 1707901427;

    address constant TEST_ADDRESS = 0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a;

    uint256 forkEra;
    uint256 forkEth;

    function setUp() public {
        forkEra = vm.createFork("mainnet", ERA_FORK_BLOCK);
        forkEth = vm.createFork("https://eth-mainnet.alchemyapi.io/v2/Lc7oIGYeL_QvInzI0Wiu_pOZZDEKBrdf", ETH_FORK_BLOCK);
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
        vm.selectFork(forkEra);
        require(TEST_ADDRESS.balance == 0, "era balance mismatch");

        vm.deal(TEST_ADDRESS, 100);
        require(TEST_ADDRESS.balance == 100, "era balance mismatch");
    }
}
