// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

import {Script} from "forge-std/Script.sol";
import {Greeter} from "../src/Greeter.sol";

contract DeployScript is Script {
    // Vm constant vm = Vm(HEVM_ADDRESS);

    Greeter greeter;
    string greeting;

    function run() external {
        // test is using old Vm.sol interface, so we call manually
        (bool success,) = address(vm).call(abi.encodeWithSignature("zkVm(bool)", true));
        require(success, "zkVm() call failed");
        vm.startBroadcast();

        greeter = new Greeter();

        greeter.setAge(123);
        uint256 age = greeter.getAge();

        greeter.greeting("john");

        vm.stopBroadcast();

        assert(age == 123);
    }
}
