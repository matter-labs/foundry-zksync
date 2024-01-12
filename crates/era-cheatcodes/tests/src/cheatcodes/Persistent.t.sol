// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "forge-std/Test.sol";
import {Constants} from "./Constants.sol";

contract Target {
    function output() public pure returns (uint256) {
        return 255;
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

contract ForkPersistentTest is Test {
    uint256 constant FORK_BLOCK = 19579636;
    address constant DUMMY_ADDRESS = 0x1345df6d4C9c3035724fd0e3914dE96a5a83AAF4;

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

        require(dummy.val() == expectedValue, "should be expected value");

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
