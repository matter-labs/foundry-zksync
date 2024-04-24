// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.7 <0.9.0;

import 'forge-std/Script.sol';
import '@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol';

contract ProxyScript {
     function run() public {
        //deploy Foo
        ERC1967Proxy proxy = new ERC1967Proxy(address(new Foo()), "");

        Foo foo = Foo(payable(proxy));
        foo.initialize(msg.sender);

        console.log("Foo deployed at: ", address(foo));
        console.log("Bar: ", foo.getAddress());
    }
}

contract Foo {
    address bar;

    function initialize(address _bar) public {
        bar = _bar;
    }

    function getAddress() public returns (address) {
        return bar;
    }
}
