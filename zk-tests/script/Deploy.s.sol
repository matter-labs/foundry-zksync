// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

import {Script} from "forge-std/Script.sol";
import {Test} from "forge-std/Test.sol";
import {console2 as console} from "forge-std/console2.sol";

contract Greeter {
    string name;
    uint256 age;

    event Greet(string greet);

    function greeting(string memory _name) public returns (string memory) {
        name = _name;
        string memory greet = string(abi.encodePacked("Hello ", _name));
        emit Greet(greet);
        return greet;
    }

    function greeting2(
        string memory _name,
        uint256 n
    ) public returns (uint256) {
        name = _name;
        string memory greet = string(abi.encodePacked("Hello ", _name));
        console.log(name);
        emit Greet(greet);
        return n * 2;
    }

    function setAge(uint256 _age) public {
        age = _age;
    }

    function getAge() public view returns (uint256) {
        return age;
    }
}

contract DeployScript is Script {
    // Vm constant vm = Vm(HEVM_ADDRESS);

    Greeter greeter;
    string greeting;

    function run() external {
        // test is using old Vm.sol interface, so we call manually
        (bool success, ) = address(vm).call(
            abi.encodeWithSignature("zkVm(bool)", true)
        );
        require(success, "zkVm() call failed");
        vm.startBroadcast();
        greeter = new Greeter();
        greeter.greeting("john");
        greeter.setAge(123);
        vm.stopBroadcast();
    }
}
