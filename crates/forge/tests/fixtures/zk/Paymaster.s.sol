// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Script, console} from "forge-std/Script.sol";
import {Greeter} from "../src/Greeter.sol";
import {MyPaymaster} from "../src/MyPaymaster.sol";

contract PaymasterScript is Script {
    MyPaymaster private paymaster;
    bytes private paymaster_encoded_input;
    address private alice;

    function run() public {
        // random private key to get the address of alice with zero balance
        alice = vm.rememberKey(0x60d80818010eb4826dc44d7342076a36978544fc89199061f452ea65d67e99e1);
        require(address(alice).balance == 0, "Alice balance is not 0");

        // We broadcast first to deploy the paymaster and fund it
        vm.startBroadcast();
        paymaster = new MyPaymaster();
        (bool transferSuccess,) = address(paymaster).call{value: 10 ether}("");
        require(transferSuccess, "Paymaster funding failed");
        vm.stopBroadcast();

        // Encode paymaster input
        paymaster_encoded_input = abi.encodeWithSelector(bytes4(keccak256("general(bytes)")), bytes("0x"));

        // We broadcast the transaction from alice's account to avoid having balance
        vm.startBroadcast(alice);

        (bool success,) = address(vm).call(
            abi.encodeWithSignature("zkUsePaymaster(address,bytes)", address(paymaster), paymaster_encoded_input)
        );
        require(success, "zkUsePaymaster call failed");

        Greeter greeter = new Greeter();

        vm.stopBroadcast();
    }
}
