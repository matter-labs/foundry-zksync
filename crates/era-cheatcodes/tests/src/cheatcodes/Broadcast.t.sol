// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.18;

import {Test, console2 as console} from "../../lib/forge-std/src/Test.sol";
import {Constants} from "./Constants.sol";

interface Cheatcodes {
    function stopBroadcast() external;
    function startBroadcast() external;
    function startBroadcast(address who) external;
    function startBroadcast(uint256 privateKey) external;
}

contract ATest is Test {
    uint256 public changed = 0;

    function t(uint256 a) public returns (uint256) {
        uint256 b = 0;
        for (uint256 i; i < a; i++) {
            b += 1;
        }
        emit log_string("here");
        return b;
    }

    function inc() public returns (uint256) {
        changed += 1;
    }

    function multiple_arguments(uint256 a, address b, uint256[] memory c) public returns (uint256) {}

    function echoSender() public view returns (address) {
        return msg.sender;
    }
}

contract BroadcastTest is Test {
    Cheatcodes constant cheatcodes = Cheatcodes(Constants.CHEATCODE_ADDRESS);

    // 1st anvil account
    address public ACCOUNT_A = 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266;
    // 2nd anvil account
    address public ACCOUNT_B = 0x70997970C51812dc3A010C7d01b50e0d17dc79C8;

    function test_deploy() public {
        cheatcodes.startBroadcast(ACCOUNT_A);
        ATest test = new ATest();
        cheatcodes.stopBroadcast();

        // this wont generate tx to sign
        uint256 b = test.t(4);

        // this will
        cheatcodes.startBroadcast(ACCOUNT_B);
        test.t(2);
        cheatcodes.stopBroadcast();
    }

    // function deployPrivateKey() public {
    //     string memory mnemonic = "test test test test test test test test test test test junk";

    //     uint256 privateKey = cheatcodes.deriveKey(mnemonic, 3);
    //     assertEq(privateKey, 0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6);

    //     cheatcodes.broadcast(privateKey);
    //     ATest test = new ATest();

    //     cheatcodes.startBroadcast(privateKey);
    //     ATest test2 = new ATest();
    //     cheatcodes.stopBroadcast();
    // }

    // function deployRememberKey() public {
    //     string memory mnemonic = "test test test test test test test test test test test junk";

    //     uint256 privateKey = cheatcodes.deriveKey(mnemonic, 3);
    //     assertEq(privateKey, 0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6);

    //     address thisAddress = cheatcodes.rememberKey(privateKey);
    //     assertEq(thisAddress, 0x90F79bf6EB2c4f870365E785982E1f101E93b906);

    //     cheatcodes.broadcast(thisAddress);
    //     ATest test = new ATest();
    // }

    // function deployRememberKeyResume() public {
    //     cheatcodes.broadcast(ACCOUNT_A);
    //     ATest test = new ATest();

    //     string memory mnemonic = "test test test test test test test test test test test junk";

    //     uint256 privateKey = cheatcodes.deriveKey(mnemonic, 3);
    //     address thisAddress = cheatcodes.rememberKey(privateKey);

    //     cheatcodes.broadcast(thisAddress);
    //     ATest test2 = new ATest();
    // }

    function test_deployOther() public {
        cheatcodes.startBroadcast(ACCOUNT_A);
        ATest tmptest = new ATest();
        ATest test = new ATest();

        // won't trigger a transaction: staticcall
        test.changed();

        // won't trigger a transaction: staticcall
        require(test.echoSender() == ACCOUNT_A);

        // will trigger a transaction
        test.t(1);

        // will trigger a transaction
        test.inc();

        cheatcodes.stopBroadcast();

        require(test.echoSender() == address(this));

        cheatcodes.startBroadcast(ACCOUNT_B);
        ATest tmptest2 = new ATest();

        // will trigger a transaction
        test.t(2);

        // will trigger a transaction from B
        payable(ACCOUNT_A).transfer(2);

        // will trigger a transaction
        test.inc();
        cheatcodes.stopBroadcast();

        assert(test.changed() == 2);
    }

    function testFail_deployPanics() public {
        cheatcodes.startBroadcast(address(0x1337));
        ATest test = new ATest();
        cheatcodes.stopBroadcast();

        // This panics because this would cause an additional relinking that isnt conceptually correct
        // from a solidity standpoint. Basically, this contract `BroadcastTest`, injects the code of
        // `ATest` *into* its code. So it isn't reasonable to break solidity to our will of having *two*
        // versions of `ATest` based on the sender/linker.
        cheatcodes.startBroadcast(address(0x1338));
        new ATest();

        test.t(0);
        cheatcodes.stopBroadcast();
    }

    function test_deployNoArgs() public {
        cheatcodes.startBroadcast();
        ATest test1 = new ATest();

        ATest test2 = new ATest();
        cheatcodes.stopBroadcast();
    }

    function testFail_NoBroadcast() public {
        cheatcodes.stopBroadcast();
    }
}

contract NoLink is Test {
    function t(uint256 a) public returns (uint256) {
        uint256 b = 0;
        for (uint256 i; i < a; i++) {
            b += i;
        }
        emit log_string("here");
        return b;
    }

    function view_me() public pure returns (uint256) {
        return 1337;
    }
}

interface INoLink {
    function t(uint256 a) external returns (uint256);
}

contract BroadcastTestNoLinking is Test {
    Cheatcodes constant cheatcodes = Cheatcodes(Constants.CHEATCODE_ADDRESS);

    // ganache-cli -d 1st
    address public ACCOUNT_A = 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266;

    // ganache-cli -d 2nd
    address public ACCOUNT_B = 0x70997970C51812dc3A010C7d01b50e0d17dc79C8;

    function test_deployDoesntPanic() public {
        cheatcodes.startBroadcast(address(ACCOUNT_A));
        NoLink test = new NoLink();
        cheatcodes.stopBroadcast();

        cheatcodes.startBroadcast(address(ACCOUNT_B));
        new NoLink();

        test.t(0);
        cheatcodes.stopBroadcast();
    }

    function test_deployMany() public {
        // assert(cheatcodes.getNonce(msg.sender) == 0);

        cheatcodes.startBroadcast();

        for (uint256 i; i < 25; i++) {
            NoLink test9 = new NoLink();
        }

        cheatcodes.stopBroadcast();
    }

    function test_deployCreate2() public {
        cheatcodes.startBroadcast();
        NoLink test_c2 = new NoLink{salt: bytes32(uint256(1337))}();
        assert(test_c2.view_me() == 1337);
        NoLink test2 = new NoLink();
        cheatcodes.stopBroadcast();
    }

    function test_errorStaticCall() public {
        cheatcodes.startBroadcast();
        NoLink test11 = new NoLink();

        test11.view_me();
        cheatcodes.stopBroadcast();
    }
}

contract BroadcastMix is Test {
    Cheatcodes constant cheatcodes = Cheatcodes(Constants.CHEATCODE_ADDRESS);

    // ganache-cli -d 1st
    address public ACCOUNT_A = 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266;

    // ganache-cli -d 2nd
    address public ACCOUNT_B = 0x70997970C51812dc3A010C7d01b50e0d17dc79C8;

    function more() internal {
        cheatcodes.startBroadcast();
        NoLink test11 = new NoLink();
        cheatcodes.stopBroadcast();
    }

    function test_deployMix() public {
        address user = msg.sender;
        assert(user == address(0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266));

        NoLink no = new NoLink();

        cheatcodes.startBroadcast();
        NoLink test1 = new NoLink();
        test1.t(2);
        NoLink test2 = new NoLink();
        test2.t(2);
        cheatcodes.stopBroadcast();

        cheatcodes.startBroadcast(user);
        NoLink test3 = new NoLink();
        NoLink test4 = new NoLink();
        test4.t(2);
        cheatcodes.stopBroadcast();

        cheatcodes.startBroadcast();
        test4.t(2);

        NoLink test5 = new NoLink();

        INoLink test6 = INoLink(address(new NoLink()));

        NoLink test7 = new NoLink();
        cheatcodes.stopBroadcast();

        cheatcodes.startBroadcast(user);
        NoLink test8 = new NoLink();
        cheatcodes.stopBroadcast();

        cheatcodes.startBroadcast();
        NoLink test9 = new NoLink();
        cheatcodes.stopBroadcast();

        cheatcodes.startBroadcast(user);
        NoLink test10 = new NoLink();
        cheatcodes.stopBroadcast();

        more();
    }
}

contract BroadcastTestLog is Test {
    Cheatcodes constant cheatcodes = Cheatcodes(Constants.CHEATCODE_ADDRESS);

    function test_logs() public {
        uint256[] memory arr = new uint256[](2);
        arr[0] = 3;
        arr[1] = 4;

        cheatcodes.startBroadcast();
        {
            ATest c1 = new ATest();
            ATest c2 = new ATest{salt: bytes32(uint256(1337))}();

            c1.multiple_arguments(1, address(0x1337), arr);
            c1.inc();
            c2.t(1);

            payable(address(0x1337)).transfer(0.0001 ether);
        }
        cheatcodes.stopBroadcast();
    }
}

contract TestInitialBalance is Test {
    Cheatcodes constant cheatcodes = Cheatcodes(Constants.CHEATCODE_ADDRESS);

    function test_customCaller() public {
        // Make sure we're testing a different caller than the default one.
        assert(msg.sender != address(0x00a329c0648769A73afAc7F9381E08FB43dBEA72));

        // NodeConfig::test() sets the balance of the address used in this test to 100 ether.
        assert(msg.sender.balance == 100 ether);

        cheatcodes.startBroadcast();
        new NoLink();
        cheatcodes.stopBroadcast();
    }

    function test_defaultCaller() public {
        // Make sure we're testing with the default caller.
        assert(msg.sender == address(0x1804c8AB1F12E6bbf3894d4083f33e07309d1f38));

        assert(msg.sender.balance == type(uint256).max);

        cheatcodes.startBroadcast();
        new NoLink();
        cheatcodes.stopBroadcast();
    }
}

// contract MultiChainBroadcastNoLink is Test {
//     Cheatcodes constant cheatcodes = Cheatcodes(Constants.CHEATCODE_ADDRESS);

//     // ganache-cli -d 1st
//     address public ACCOUNT_A = 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266;

//     // ganache-cli -d 2nd
//     address public ACCOUNT_B = 0x70997970C51812dc3A010C7d01b50e0d17dc79C8;

//     function deploy(string memory sforkA, string memory sforkB) public {
//         uint256 forkA = cheatcodes.createFork(sforkA);
//         uint256 forkB = cheatcodes.createFork(sforkB);

//         cheatcodes.selectFork(forkA);
//         cheatcodes.broadcast(address(ACCOUNT_A));
//         new NoLink();
//         cheatcodes.broadcast(address(ACCOUNT_B));
//         new NoLink();
//         cheatcodes.selectFork(forkB);
//         cheatcodes.startBroadcast(address(ACCOUNT_B));
//         new NoLink();
//         new NoLink();
//         new NoLink();
//         cheatcodes.stopBroadcast();
//         cheatcodes.startBroadcast(address(ACCOUNT_A));
//         new NoLink();
//         new NoLink();
//     }

//     function deployError(string memory sforkA, string memory sforkB) public {
//         uint256 forkA = cheatcodes.createFork(sforkA);
//         uint256 forkB = cheatcodes.createFork(sforkB);

//         cheatcodes.selectFork(forkA);
//         cheatcodes.broadcast(address(ACCOUNT_A));
//         new NoLink();
//         cheatcodes.startBroadcast(address(ACCOUNT_B));
//         new NoLink();

//         cheatcodes.selectFork(forkB);
//         cheatcodes.broadcast(address(ACCOUNT_B));
//         new NoLink();
//     }
// }

// contract MultiChainBroadcastLink is Test {
//     Cheatcodes constant cheatcodes = Cheatcodes(Constants.CHEATCODE_ADDRESS);

//     // ganache-cli -d 1st
//     address public ACCOUNT_A = 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266;

//     // ganache-cli -d 2nd
//     address public ACCOUNT_B = 0x70997970C51812dc3A010C7d01b50e0d17dc79C8;

//     function deploy(string memory sforkA, string memory sforkB) public {
//         uint256 forkA = cheatcodes.createFork(sforkA);
//         uint256 forkB = cheatcodes.createFork(sforkB);

//         cheatcodes.selectFork(forkA);
//         cheatcodes.broadcast(address(ACCOUNT_B));
//         new ATest();

//         cheatcodes.selectFork(forkB);
//         cheatcodes.broadcast(address(ACCOUNT_B));
//         new ATest();
//     }
// }

contract ContractA {
    uint256 var1;

    constructor(address script_caller) {
        require(msg.sender == script_caller);
        require(tx.origin == script_caller);
    }

    function method(address script_caller) public {
        require(msg.sender == script_caller);
        require(tx.origin == script_caller);
    }
}

contract ContractB {
    uint256 var2;

    constructor(address script_caller) {
        require(address(0x1337) != script_caller);
        require(msg.sender == address(0x1337));
        require(tx.origin == address(0x1337));
    }

    function method(address script_caller) public {
        require(address(0x1337) != script_caller);
        require(msg.sender == address(0x1337));
        require(tx.origin == address(0x1337));
    }
}

contract CheckOverrides is Test {
    Cheatcodes constant cheatcodes = Cheatcodes(Constants.CHEATCODE_ADDRESS);

    function test_checkOverrides() external {
        // `script_caller` can be set by `--private-key ...` or `--sender ...`
        // Otherwise it will take the default value of 0x1804c8AB1F12E6bbf3894d4083f33e07309d1f38
        address script_caller = msg.sender;
        require(script_caller == 0x1804c8AB1F12E6bbf3894d4083f33e07309d1f38);
        require(tx.origin == script_caller);

        // startBroadcast(script_caller)
        cheatcodes.startBroadcast();
        require(tx.origin == script_caller);
        require(msg.sender == script_caller);

        ContractA a = new ContractA(script_caller);
        require(tx.origin == script_caller);
        require(msg.sender == script_caller);

        a.method(script_caller);
        require(tx.origin == script_caller);
        require(msg.sender == script_caller);

        cheatcodes.stopBroadcast();

        // startBroadcast(msg.sender)
        cheatcodes.startBroadcast(address(0x1337));
        require(tx.origin == script_caller);
        require(msg.sender == script_caller);
        require(msg.sender != address(0x1337));

        ContractB b = new ContractB(script_caller);
        require(tx.origin == script_caller);
        require(msg.sender == script_caller);

        b.method(script_caller);
        require(tx.origin == script_caller);
        require(msg.sender == script_caller);

        cheatcodes.stopBroadcast();
    }
}

contract Child {}

contract Parent {
    constructor() {
        new Child();
    }
}

contract ScriptAdditionalContracts is Test {
    Cheatcodes constant cheatcodes = Cheatcodes(Constants.CHEATCODE_ADDRESS);

    function test_additionalContracts() external {
        cheatcodes.startBroadcast();
        new Parent();
    }
}
