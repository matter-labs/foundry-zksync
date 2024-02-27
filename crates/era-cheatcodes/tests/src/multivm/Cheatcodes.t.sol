// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";

contract MultiVmCheatcodeTest is Test {
    
    /*
    contract Number {
        function ten() public pure returns (uint8) {
            return 10;
        }
    }
    */
    bytes constant NUMBER_CODE =
        hex"6080604052348015600f57600080fd5b506004361060285760003560e01c8063643ceff914602d575b600080fd5b60408051600a815290519081900360200190f3fea2646970667358221220fb510bd1eb01b9ab2dcd2b21415ec03c4883eb5d31eecc64b1f78ccf455f2d0964736f6c63430008160033";
    address constant TEST_ADDRESS = 0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a;

    uint256 constant ERA_FORK_BLOCK = 19579636;
    uint256 constant ETH_FORK_BLOCK = 19225195;

    uint256 forkEra;
    uint256 forkEth;

    function setUp() public {
        forkEra = vm.createFork("mainnet", ERA_FORK_BLOCK);
        forkEth = vm.createFork(
            "https://eth-mainnet.alchemyapi.io/v2/Lc7oIGYeL_QvInzI0Wiu_pOZZDEKBrdf",
            ETH_FORK_BLOCK
        );
    }

    function testCheatcodeRoll() public {
        vm.selectFork(forkEra);
        vm.roll(1000);
        require(block.number == 1000, "era vm.roll failed");

        vm.selectFork(forkEth);
        vm.roll(1001);
        require(block.number == 1001, "eth vm.roll failed");

        vm.selectFork(forkEra);
    }

    function testCheatcodeWarp() public {
        vm.selectFork(forkEra);
        vm.warp(1000);
        require(block.timestamp == 1000, "era vm.warp failed");

        vm.selectFork(forkEth);
        vm.warp(1001);
        require(block.timestamp == 1001, "eth vm.warp failed");

        vm.selectFork(forkEra);
    }

    function testCheatcodeDeal() public {
        vm.selectFork(forkEra);
        vm.deal(TEST_ADDRESS, 1000);
        require(address(TEST_ADDRESS).balance == 1000, "era vm.deal failed");

        vm.selectFork(forkEth);
        vm.deal(TEST_ADDRESS, 1001);
        require(address(TEST_ADDRESS).balance == 1001, "eth vm.deal failed");

        vm.selectFork(forkEra);
    }

    function testCheatcodeSetNonce() public {
        vm.selectFork(forkEra);
        vm.setNonce(TEST_ADDRESS, 10);
        require(vm.getNonce(TEST_ADDRESS) == 10, "era vm.deal failed");

        vm.selectFork(forkEth);
        vm.setNonce(TEST_ADDRESS, 11);
        require(vm.getNonce(TEST_ADDRESS) == 11, "eth vm.deal failed");

        vm.selectFork(forkEra);
    }

    function testCheatcodeEtch() public {
        vm.selectFork(forkEra);
        require(block.number == ERA_FORK_BLOCK, "era vm.roll failed");

        vm.selectFork(forkEth);
        vm.etch(TEST_ADDRESS, NUMBER_CODE);
        (bool success, bytes memory data) = TEST_ADDRESS.call(abi.encodeWithSignature("ten()"));
        require(success);
        require(keccak256(data) == keccak256(abi.encode(10)), "eth vm.etch failed");

        vm.selectFork(forkEra);
    }
}
