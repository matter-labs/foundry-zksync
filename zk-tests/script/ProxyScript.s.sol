// SPDX-License-Identifier: UNLICENSED
pragma solidity >=0.8.7 <0.9.0;

import 'forge-std/Script.sol';
import '@openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol';

abstract contract BaseScript is Script {
    address public deployer;

    modifier broadcaster() {
        vm.startBroadcast(deployer);
        _;
        vm.stopBroadcast();
    }

    function setUp() public virtual {
        deployer = vm.rememberKey(vm.envUint("PRIVATE_KEY"));
    }
}

contract ProxyScript is BaseScript {
     function run() public broadcaster {
        console.log(address(deployer));

        //deploy Foo
        ERC1967Proxy proxy = new ERC1967Proxy(address(new Foo()), "");

        Foo foo = Foo(payable(proxy));
        foo.initialize(deployer);

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
