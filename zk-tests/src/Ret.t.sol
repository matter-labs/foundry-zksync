// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "forge-std/Test.sol";

contract Number {
    function one() public pure returns (uint256) {
        return 1;
    }

    function truth() public pure returns (bool) {
        return false;
    }

    function none() public pure {
        return;
    }
}

contract NumberFactory {
    function oneFactory(Number number) public pure returns (uint256) {
        return number.one();
    }

    function truthFactory(Number number) public pure returns (bool) {
        return number.truth();
    }

    function noneFactory(Number number) public pure {
        return number.none();
    }
}


contract RetTest is Test {
    // uint256 constant ERA_FORK_BLOCK = 19579636;
    // uint256 constant ERA_FORK_BLOCK_TS = 1700601590;

    // uint256 forkEra;

    // function setUp() public {
    //     forkEra = vm.createFork("mainnet", ERA_FORK_BLOCK);
    // }

    function testRet() public {
        // (bool success, ) = address(vm).call(
        //     abi.encodeWithSignature("zkVm(bool)", true)
        // );
        // require(success, "zkVm() call failed");

        // vm.deal(address(0xabcdef), 10);

        Number target = new Number();
        NumberFactory target1 = new NumberFactory();
        vm.mockCall(address(target), abi.encodeWithSelector(target.one.selector), abi.encode(12));
        // vm.mockCall(address(target), abi.encodeWithSelector(target.truth.selector), abi.encode(true));
        // vm.mockCall(address(target), abi.encodeWithSelector(target.none.selector), abi.encode());
        // target.one();
        // console.log(target.one());
        // console.log();
        console.log(target1.oneFactory(target));
        // console.log(target1.truthFactory(target));
        // target1.noneFactory(target);
        // vm.mockCall(address(target), abi.encodeWithSelector(target.one.selector), abi.encode(11));
        // assertEq(11, target.one());
    }
}
