// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import {Test, console2 as console} from "forge-std/Test.sol";
import {ConstantNumber} from "./ConstantNumber.sol";
import {Greeter} from "./Greeter.sol";

interface ISystemContractDeployer {
    function getNewAddressCreate2(
        address _sender,
        bytes32 _bytecodeHash,
        bytes32 _salt,
        bytes calldata _input
    ) external view returns (address newAddress);
}

contract Number {
    function ten() public pure returns (uint8) {
        return 10;
    }
}

contract FixedNumber {
    function five() public pure returns (uint8) {
        return 5;
    }
}

contract FixedGreeter {
    function greet(string memory _name) public pure returns (string memory) {
        string memory greeting = string(abi.encodePacked("Hello ", _name));
        return greeting;
    }
}

contract MultiNumber {
    function one() public pure returns (uint8) {
        return 1;
    }

    function two() public pure returns (uint8) {
        return 2;
    }
}

contract PayableFixedNumber {
    address sender;
    uint256 value;
    receive() external payable {}

    constructor() payable {
        sender = msg.sender;
        value = msg.value;
        console.log(msg.value);
    }

    function transfer(address payable dest, uint256 amt) public {
        (bool success, ) = dest.call{value: amt}("");
        require(success);
    }

    function five() public pure returns (uint8) {
        return 5;
    }
}

contract CustomNumber {
    uint8 value;

    constructor(uint8 _value) {
        value = _value;
    }

    function number() public view returns (uint8) {
        return value;
    }
}

contract CustomStorage {
    uint8 num;
    string str;

    constructor(string memory _str, uint8 _num) {
        str = _str;
        num = _num;
    }

    function getStr() public view returns (string memory) {
        return str;
    }

    function getNum() public view returns (uint8) {
        return num;
    }
}

contract ZkContractsTest is Test {
    Number number;
    CustomNumber customNumber;
    MultiNumber multiNumber;

    uint256 constant ERA_FORK_BLOCK = 19579636;
    uint256 constant ERA_FORK_BLOCK_TS = 1700601590;

    uint256 constant ETH_FORK_BLOCK = 19225195;
    uint256 constant ETH_FORK_BLOCK_TS = 1707901427;

    uint256 forkEra;
    uint256 forkEth;

    function setUp() public {
        number = new Number();
        customNumber = new CustomNumber(20);
        multiNumber = new MultiNumber();
        vm.makePersistent(address(number));
        vm.makePersistent(address(customNumber));

        forkEra = vm.createFork("mainnet", ERA_FORK_BLOCK);
        forkEth = vm.createFork(
            "https://eth-mainnet.alchemyapi.io/v2/Lc7oIGYeL_QvInzI0Wiu_pOZZDEKBrdf",
            ETH_FORK_BLOCK
        );
    }

    function testZkContractCanCallMethod() public {
        FixedGreeter g = new FixedGreeter();
        vm.makePersistent(address(g));
        vm.selectFork(forkEra);
        assertEq("Hello hi", g.greet("hi"));
    }

    function testZkContractsPersistedDeployedContractNoArgs() public {
        require(number.ten() == 10, "base setUp contract value mismatch");

        vm.selectFork(forkEra);
        require(number.ten() == 10, "era setUp contract value mismatch");

        vm.selectFork(forkEth);
        require(number.ten() == 10, "eth setUp contract value mismatch");
    }

    function testZkContractsPersistedDeployedContractArgs() public {
        require(
            customNumber.number() == 20,
            "base setUp contract value mismatch"
        );

        vm.selectFork(forkEra);
        require(
            customNumber.number() == 20,
            "era setUp contract value mismatch"
        );

        vm.selectFork(forkEth);
        require(
            customNumber.number() == 20,
            "eth setUp contract value mismatch"
        );
    }

    function testZkContractsInlineDeployedContractNoArgs() public {
        vm.selectFork(forkEra);
        FixedNumber fixedNumber = new FixedNumber();
        require(
            fixedNumber.five() == 5,
            "era deployed contract value mismatch"
        );
    }

    function testZkContractsInlineDeployedContractBalance() public {
        vm.selectFork(forkEra);
        PayableFixedNumber payableFixedNumber = new PayableFixedNumber{
            value: 10
        }();
        require(address(payableFixedNumber).balance == 10, "incorrect balance");
    }

    function testZkContractsExpectedBalances() public {
        vm.selectFork(forkEra);
        uint balanceBefore = address(this).balance;

        PayableFixedNumber one = new PayableFixedNumber{value: 10}();

        PayableFixedNumber two = new PayableFixedNumber();
        one.transfer(payable(address(two)), 5);

        require(
            address(one).balance == 5,
            "first contract's balance not decreased"
        );
        require(
            address(two).balance == 5,
            "second contract's balance not increased"
        );
        require(
            address(this).balance == balanceBefore - 10,
            "test address balance not decreased"
        );
    }

    function testZkContractsInlineDeployedContractComplexArgs() public {
        CustomStorage customStorage = new CustomStorage("hello", 10);
        vm.makePersistent(address(customStorage));
        require(
            keccak256(abi.encodePacked(customStorage.getStr())) ==
                keccak256(abi.encodePacked("hello")),
            "base inline contract value mismatch (complex args)"
        );
        require(
            customStorage.getNum() == 10,
            "base inline contract value mismatch (complex args)"
        );

        vm.selectFork(forkEra);
        require(
            keccak256(abi.encodePacked(customStorage.getStr())) ==
                keccak256(abi.encodePacked("hello")),
            "era inline contract value mismatch (complex args)"
        );
        require(
            customStorage.getNum() == 10,
            "era inline contract value mismatch (complex args)"
        );

        vm.selectFork(forkEth);
        require(
            keccak256(abi.encodePacked(customStorage.getStr())) ==
                keccak256(abi.encodePacked("hello")),
            "eth inline contract value mismatch (complex args)"
        );
        require(
            customStorage.getNum() == 10,
            "era inline contract value mismatch (complex args)"
        );
    }

    function testZkContractsCreate2() public {
        vm.selectFork(forkEra);

        // ConstantNumber zksolc hash obtained from zkout/ConstantNumber.sol/artifacts.json
        string memory artifact = vm.readFile(
            "zkout/ConstantNumber.sol/ConstantNumber.json"
        );
        bytes32 bytecodeHash = vm.parseJsonBytes32(artifact, ".hash");
        address sender = address(0x7FA9385bE102ac3EAc297483Dd6233D62b3e1496);
        bytes32 salt = "12345";
        bytes32 constructorInputHash = keccak256(abi.encode());
        address expectedDeployedAddress = _computeCreate2Address(
            sender,
            salt,
            bytes32(bytecodeHash),
            constructorInputHash
        );

        // deploy via create2
        address actualDeployedAddress = address(
            new ConstantNumber{salt: salt}()
        );
        assertEq(expectedDeployedAddress, actualDeployedAddress);
    }

    function _computeCreate2Address(
        address sender,
        bytes32 salt,
        bytes32 creationCodeHash,
        bytes32 constructorInputHash
    ) private pure returns (address) {
        bytes32 zksync_create2_prefix = keccak256("zksyncCreate2");
        bytes32 address_hash = keccak256(
            bytes.concat(
                zksync_create2_prefix,
                bytes32(uint256(uint160(sender))),
                salt,
                creationCodeHash,
                constructorInputHash
            )
        );

        return address(uint160(uint256(address_hash)));
    }

    function testZkContractsCallSystemContract() public {
        (bool success, ) = address(vm).call(
            abi.encodeWithSignature("zkVm(bool)", true)
        );
        require(success, "zkVm() call failed");

        ISystemContractDeployer deployer = ISystemContractDeployer(
            address(0x0000000000000000000000000000000000008006)
        );

        address addr = deployer.getNewAddressCreate2(
            address(this),
            0x0100000781e55a60f3f14fd7dd67e3c8caab896b7b0fca4a662583959299eede,
            0x0100000781e55a60f3f14fd7dd67e3c8caab896b7b0fca4a662583959299eede,
            ""
        );

        assertEq(address(0x46efB6258A2A539f7C8b44e2EF659D778fb5BAAd), addr);
    }

    function testZkContractsDeployedInSetupAreMockable() public {
        vm.mockCall(
            address(multiNumber),
            abi.encodeWithSelector(MultiNumber.one.selector),
            abi.encode(42)
        );

        assertEq(42, multiNumber.one());
        assertEq(2, multiNumber.two());
    }

    function testZkStaticCalls() public {
        (bool success, ) = address(vm).call(
            abi.encodeWithSignature("zkVm(bool)", true)
        );
        require(success, "zkVm() call failed");
        address sender = address(this);
        uint64 startingNonce = vm.getNonce(sender);

        //this ensures calls & deployments increase the nonce
        vm.startBroadcast(sender);

        Greeter greeter = new Greeter();
        assert(vm.getNonce(sender) == startingNonce + 1);

        greeter.setAge(42);
        assert(vm.getNonce(sender) == startingNonce + 2);

        // static-call, nonce shouldn't change
        uint256 age = greeter.getAge();
        assert(age == 42);
        assert(vm.getNonce(sender) == startingNonce + 2);

        uint256 num = greeter.greeting2("zksync", 2);
        assert(num == 4);
        assert(vm.getNonce(sender) == startingNonce + 3);
        vm.stopBroadcast();
    }
}
