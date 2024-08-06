// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

import "ds-test/test.sol";
import "../cheats/Vm.sol";
import {Globals} from "./Globals.sol";

contract BlockEnv {
    uint256 public number;
    uint256 public timestamp;
    uint256 public basefee;
    uint256 public chainid;

    constructor() {
        number = block.number;
        timestamp = block.timestamp;
        basefee = block.basefee;
        chainid = block.chainid;
    }

    function zkBlockhash(uint256 _blockNumber) public view returns (bytes32) {
        return blockhash(_blockNumber);
    }
}

contract ZkBasicTest is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);

    uint256 constant ERA_FORK_BLOCK = 19579636;
    uint256 constant ERA_FORK_BLOCK_TS = 1700601590;

    uint256 constant ETH_FORK_BLOCK = 19225195;
    uint256 constant ETH_FORK_BLOCK_TS = 1707901427;

    address constant TEST_ADDRESS = 0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a;

    uint256 forkEra;
    uint256 forkEth;
    uint256 latestForkEth;

    function setUp() public {
        forkEra = vm.createFork(Globals.ZKSYNC_MAINNET_URL, ERA_FORK_BLOCK);
        forkEth = vm.createFork(Globals.ETHEREUM_MAINNET_URL, ETH_FORK_BLOCK);
        latestForkEth = vm.createFork(Globals.ETHEREUM_MAINNET_URL);
    }

    function testZkBasicBlockNumber() public {
        vm.selectFork(forkEra);
        require(block.number == ERA_FORK_BLOCK, "era block number mismatch");

        vm.selectFork(forkEth);
        require(block.number == ETH_FORK_BLOCK, "eth block number mismatch");
    }

    function testZkBasicBlockTimestamp() public {
        vm.selectFork(forkEra);
        require(block.timestamp == ERA_FORK_BLOCK_TS, "era block timestamp mismatch");

        vm.selectFork(forkEth);
        require(block.timestamp == ETH_FORK_BLOCK_TS, "eth block timestamp mismatch");
    }

    function testZkBasicAddressBalance() public {
        vm.makePersistent(TEST_ADDRESS);
        vm.deal(TEST_ADDRESS, 100);

        vm.selectFork(forkEra);
        require(TEST_ADDRESS.balance == 100, "era balance mismatch");

        vm.selectFork(forkEth);
        require(TEST_ADDRESS.balance == 100, "eth balance mismatch");
    }

    function testZkPropagatedBlockEnv() public {
        BlockEnv be = new BlockEnv();
        require(be.number() == block.number, "propagated block number is the same as current");
        require(be.timestamp() == block.timestamp, "propagated block timestamp is the same as current");
        require(be.basefee() == block.basefee, "propagated block basefee is the same as current");
        require(be.chainid() == block.chainid, "propagated block chainid is the same as current");

        require(
            be.zkBlockhash(block.number) == blockhash(block.number), "blockhash of the current block should be zero"
        );

        // this corresponds to the the genesis block since the test runs in block #1
        require(
            be.zkBlockhash(block.number - 1) == blockhash(block.number - 1),
            "blockhash of the previous block should be equal"
        );

        require(be.zkBlockhash(0) == blockhash(0), "blockhash of the genesis block should be equal");

        be = new BlockEnv();
        require(be.number() == block.number, "propagated block number stays constant");
        require(be.timestamp() == block.timestamp, "propagated block timestamp stays constant");
        require(be.basefee() == block.basefee, "propagated block basefee stays constant");
        require(be.chainid() == block.chainid, "propagated block chainid stays constant");

        vm.roll(42);
        vm.warp(42);

        be = new BlockEnv();
        require(be.number() == block.number, "propagated block number rolls");
        require(be.timestamp() == block.timestamp, "propagated block timestamp warps");
        require(be.basefee() == block.basefee, "propagated block basefee warps");
    }

    function testZkBasicBlockBaseFee() public {
        BlockEnv beBefore = new BlockEnv();
        require(beBefore.basefee() == block.basefee, "propagated block basefee is the same as current");

        vm.selectFork(forkEra);
        BlockEnv beAfter = new BlockEnv();
        require(beAfter.basefee() == block.basefee, "propagated block basefee is the same as before");
        require(beAfter.basefee() == block.basefee, "propagated block basefee is the same as before");
    }

    function testZkBlockHashWithNewerBlocks() public {
        vm.selectFork(latestForkEth);
        BlockEnv be = new BlockEnv();
        require(be.zkBlockhash(block.number) == blockhash(block.number), "blockhash mismatch");
    }
}
