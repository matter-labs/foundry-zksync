// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

contract ForkTest is Test {
    /// USDC TOKEN
    address constant TOKEN_ADDRESS = 0x3355df6D4c9C3035724Fd0e3914dE96A5a83aaf4;
    uint256 constant TOKEN_DECIMALS = 6;
    uint256 constant FORK_BLOCK = 19579636;
    address constant DUMMY_ADDRESS = 0x1345df6d4C9c3035724fd0e3914dE96a5a83AAF4;

    function setUp() public {
        /// USDC TOKEN doesn't exists locally
        (bool success, bytes memory data) = TOKEN_ADDRESS.call(
            abi.encodeWithSignature("decimals()")
        );
        require(success, "decimals() failed");
        uint256 decimals_before = uint256(bytes32(data));
        require(
            block.number < 1000,
            "Local node doesn't have blocks above 1000"
        );

        vm.createSelectFork("mainnet", FORK_BLOCK);

        require(decimals_before == 0, "Contract exists locally");
    }

    function testFork() public {
        /// After createSelect fork the decimals should exist
        (bool success, bytes memory data2) = TOKEN_ADDRESS.call(
            abi.encodeWithSignature("decimals()")
        );
        require(success, "decimals() failed");
        uint256 decimals_after = uint256(bytes32(data2));
        console.log("decimals_after", decimals_after);
        require(
            decimals_after == TOKEN_DECIMALS,
            "Contract doesn't exists in fork"
        );
        require(
            block.number == FORK_BLOCK + 1,
            "ENV for blocks is not set correctly"
        );
    }

    function testCreateSelectFork() public {
        uint256 forkId = vm.createFork("mainnet", FORK_BLOCK + 100);

        vm.selectFork(forkId);

        /// After createSelect fork the decimals  should exist
        (bool success2, bytes memory data2) = TOKEN_ADDRESS.call(
            abi.encodeWithSignature("decimals()")
        );
        require(success2, "decimals() failed");
        uint256 decimals_after = uint256(bytes32(data2));
        console.log("decimals_after", decimals_after);
        console.log("block ", block.number);
        require(
            decimals_after == TOKEN_DECIMALS,
            "Contract dosent exists in fork"
        );
        require(
            block.number == FORK_BLOCK + 100,
            "ENV for blocks is not set correctly"
        );
    }

    function testActiveFork() public {
        uint256 data = vm.createFork("mainnet", FORK_BLOCK + 100);

        uint256 forkId = uint256(bytes32(data));
        vm.selectFork(forkId);

        uint256 activeFork = vm.activeFork();

        require(activeFork == forkId, "Active fork is not correct");
    }

    /// checks that marking as persistent works
    function testMakePersistent() public {
        uint256 mainnetFork = vm.createFork("mainnet", FORK_BLOCK + 100);
        uint256 optimismFork = vm.createFork("mainnet", FORK_BLOCK + 100);

       DummyContract dummy = new DummyContract();

        require(
            vm.isPersistent(address(dummy)) == false,
            "should not be persistent"
        );

        vm.selectFork(mainnetFork);

        uint256 expectedValue = 99;
        dummy.set(expectedValue);

        vm.selectFork(optimismFork);

        vm.selectFork(mainnetFork);

        require(dummy.val() == expectedValue, "should be expected value"));

        vm.makePersistent(address(dummy));
        require(
            vm.isPersistent(address(dummy)) == true,
            "should be persistent"
        );

        vm.selectFork(optimismFork);
        // the account is now marked as persistent and the contract is persistent across swaps
        dummy.hello();
        require(dummy.val() == expectedValue, "should be expected value");
    }

    function testRevokePersistent() public {
        // the dummy address should not be persistent
        require(
            vm.isPersistent(DUMMY_ADDRESS) == false,
            "should not be persistent"
        );

        // mark the dummy address as persistent
        vm.makePersistent(DUMMY_ADDRESS);

        // the dummy address should now be persistent
        require(vm.isPersistent(DUMMY_ADDRESS) == true, "should be persistent");

        // revoke the dummy address as persistent
        vm.revokePersistent(DUMMY_ADDRESS);

        // the dummy address should not be persistent anymore
        require(
            vm.isPersistent(DUMMY_ADDRESS) == false,
            "should not be persistent"
        );
    }
}

contract DummyContract {
    uint256 public val;

    function hello() external view returns (string memory) {
        return "hello";
    }

    function set(uint256 _val) public {
        val = _val;
    }
}
