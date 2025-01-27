// SPDX-License-Identifier: UNLICENSED

pragma solidity >=0.8.7 <0.9.0;

import {UsesFoo} from "../src/WithLibraries.sol";
import "forge-std/Script.sol";

contract GetCodeUnlinked is Script {
   function run() external {
       // should fail because `UsesFoo` is unlinked
       bytes memory _code = vm.getCode("UsesFoo");
   }
}

contract DeployTimeLinking is Script {
    function run() external {
        vm.broadcast();
        UsesFoo user = new UsesFoo();

        assert(user.number() == 42);
    }
}
