// SPDX-License-Identifier: MIT OR Apache-2.0
pragma solidity ^0.8.18;

import "ds-test/test.sol";
import "cheats/Vm.sol";
import {Globals} from "../Globals.sol";

import "../../default/logs/console.sol";

contract Counter {
    uint256 public number;

    function inc() public {
        number += 1;
    }

    function reset() public {
        number = 0;
    }
}

contract CounterHandler is DSTest {
   Vm constant vm = Vm(HEVM_ADDRESS);

   uint256 public incCounter;
   uint256 public resetCounter;
   Counter public counter;

   constructor(Counter _counter) {
       counter = _counter;
   }

   function inc() public {
       console.log("inc");
       incCounter += 1;

       vm.deal(tx.origin, 1 ether);  // ensure caller has funds
       vm.zkVm(true);
       counter.inc();
   }

   function reset() public {
       console.log("reset");
       resetCounter += 1;

       vm.deal(tx.origin, 1 ether); // ensure caller has funds
       vm.zkVm(true);
       counter.reset();
   }
}

// partial from forge-std/StdInvariant.sol
abstract contract StdInvariant {
    struct FuzzSelector {
        address addr;
        bytes4[] selectors;
    }

    address[] internal _targetedContracts;
    function targetContracts() public view returns (address[] memory) {
        return _targetedContracts;
    }

    FuzzSelector[] internal _targetedSelectors;
    function targetSelectors() public view returns (FuzzSelector[] memory) {
        return _targetedSelectors;
    }
}

// https://github.com/matter-labs/foundry-zksync/issues/565
contract Issue565 is DSTest, StdInvariant {
    Vm constant vm = Vm(HEVM_ADDRESS);
    Counter cnt;
    CounterHandler handler;

    function setUp() public {
        cnt = new Counter();

        vm.zkVm(false);
        handler = new CounterHandler(cnt);

        // add the handler selectors to the fuzzing targets
        bytes4[] memory selectors = new bytes4[](2);
        selectors[0] = CounterHandler.inc.selector;
        selectors[1] = CounterHandler.reset.selector;

        _targetedContracts.push(address(handler));
        _targetedSelectors.push(FuzzSelector({addr: address(handler), selectors: selectors}));
    }

    //FIXME: seems to not be detected, forcing values in test config
    /// forge-config: default.invariant.fail-on-revert = true
    /// forge-config: default.invariant.no-zksync-reserved-addresses = true
    function invariant_ghostVariables() external {
        vm.zkVm(true);
        uint256 num = cnt.number();

        vm.zkVm(false);
        if (handler.resetCounter() == 0) {
            assert(handler.incCounter() == num);
        } else {
            assert(handler.incCounter() != 0);
        }
    }
}
