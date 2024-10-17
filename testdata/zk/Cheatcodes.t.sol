// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.18;

import "ds-test/test.sol";
import "../cheats/Vm.sol";
import {Globals} from "./Globals.sol";
import "../default/logs/console.sol";

contract FixedSlot {
    uint8 num; // slot index: 0

    function setSlot0(uint8 _num) public {
        num = _num;
    }
}

contract InnerMock {
    function getBytes() public payable returns (bytes memory) {
        bytes memory r = bytes(hex"abcd");
        return r;
    }
}

contract Mock {
    InnerMock private inner;

    constructor(InnerMock _inner) payable {
        inner = _inner;
    }

    function getBytes() public returns (bytes memory) {
        return inner.getBytes{value: 10}();
    }
}

interface IMyProxyCaller {
    function transact(uint8 _data) external;
}

contract MyProxyCaller {
    address inner;

    constructor(address _inner) {
        inner = _inner;
    }

    function transact() public {
        IMyProxyCaller(inner).transact(10);
    }
}

contract Emitter {
    event EventConstructor(string message);
    event EventFunction(string message);

    string public constant CONSTRUCTOR_MESSAGE = "constructor";
    string public constant FUNCTION_MESSAGE = "function";

    constructor() {
        emit EventConstructor(CONSTRUCTOR_MESSAGE);
    }

    function functionEmit() public {
        emit EventFunction(FUNCTION_MESSAGE);
    }

    function emitConsole(string memory message) public view {
        console.log(message);
    }
}

contract ZkCheatcodesTest is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);

    event EventConstructor(string message);
    event EventFunction(string message);

    uint256 testSlot = 0; //0x000000000000000000000000000000000000000000000000000000000000001e slot
    uint256 constant ERA_FORK_BLOCK = 19579636;
    uint256 constant ERA_FORK_BLOCK_TS = 1700601590;

    uint256 constant ETH_FORK_BLOCK = 19225195;
    uint256 constant ETH_FORK_BLOCK_TS = 1707901427;

    address constant TEST_ADDRESS = 0x6Eb28604685b1F182dAB800A1Bfa4BaFdBA8a79a;

    uint256 forkEra;
    uint256 forkEth;

    function setUp() public {
        forkEra = vm.createFork(Globals.ZKSYNC_MAINNET_URL, ERA_FORK_BLOCK);
        forkEth = vm.createFork(Globals.ETHEREUM_MAINNET_URL, ETH_FORK_BLOCK);
    }

    function testZkCheatcodesRoll() public {
        vm.selectFork(forkEra);
        require(block.number == ERA_FORK_BLOCK, "era block number mismatch");

        vm.roll(ERA_FORK_BLOCK + 1);
        require(block.number == ERA_FORK_BLOCK + 1, "era block number mismatch");
    }

    function testZkCheatcodesWarp() public {
        vm.selectFork(forkEra);
        require(block.timestamp == ERA_FORK_BLOCK_TS, "era block timestamp mismatch");

        vm.warp(ERA_FORK_BLOCK_TS + 1);
        require(block.timestamp == ERA_FORK_BLOCK_TS + 1, "era block timestamp mismatch");
    }

    function testZkCheatcodesDeal() public {
        vm.selectFork(forkEra);
        require(TEST_ADDRESS.balance == 0, "era balance mismatch");

        vm.deal(TEST_ADDRESS, 100);
        require(TEST_ADDRESS.balance == 100, "era balance mismatch");
    }

    function testZkCheatcodesSetNonce() public {
        vm.selectFork(forkEra);
        require(vm.getNonce(TEST_ADDRESS) == 0, "era nonce mismatch");

        vm.setNonce(TEST_ADDRESS, 10);
        require(vm.getNonce(TEST_ADDRESS) == 10, "era nonce mismatch");

        vm.resetNonce(TEST_ADDRESS);
        require(vm.getNonce(TEST_ADDRESS) == 0, "era nonce mismatch");
    }

    function testZkCheatcodesGetCode() public {
        string memory contractName = "ConstantNumber";
        getCodeCheck(contractName, "zkout");

        vm.zkVm(false);
        getCodeCheck(contractName, "out");
    }

    function testZkCheatcodesEtch() public {
        vm.selectFork(forkEra);

        string memory artifact = vm.readFile("./zk/zkout/ConstantNumber.sol/ConstantNumber.json");
        bytes memory constantNumberCode = vm.parseJsonBytes(artifact, ".bytecode.object");
        vm.etch(TEST_ADDRESS, constantNumberCode);

        (bool success, bytes memory output) = TEST_ADDRESS.call(abi.encodeWithSignature("ten()"));
        require(success, "ten() call failed");

        uint8 number = abi.decode(output, (uint8));
        require(number == 10, "era etched code incorrect");
    }

    function testRecord() public {
        FixedSlot fs = new FixedSlot();
        vm.record();
        fs.setSlot0(10);
        (bytes32[] memory reads, bytes32[] memory writes) = vm.accesses(address(fs));
        bytes32 keySlot0 = bytes32(uint256(0));
        assertEq(reads[0], keySlot0);
        assertEq(writes[0], keySlot0);
    }

    function testExpectEmit() public {
        vm.expectEmit(true, true, true, true);
        emit EventFunction("function");
        Emitter emitter = new Emitter();
        emitter.functionEmit();
    }

    function testExpectEmitOnCreate() public {
        vm.expectEmit(true, true, true, true);
        emit EventConstructor("constructor");
        new Emitter();
    }

    function testExpectEmitIgnoresStaticCalls() public {
        Emitter emitter = new Emitter();

        vm.expectEmit(true, true, true, true);
        emit EventFunction(emitter.FUNCTION_MESSAGE());
        emitter.functionEmit();
    }

    function testZkCheatcodesValueFunctionMockReturn() public {
        InnerMock inner = new InnerMock();
        // Send some funds to so it can pay for the inner call
        Mock target = new Mock{value: 50}(inner);

        bytes memory dataBefore = target.getBytes();
        assertEq(dataBefore, bytes(hex"abcd"));

        vm.mockCall(address(inner), abi.encodeWithSelector(inner.getBytes.selector), abi.encode(bytes(hex"a1b1")));

        bytes memory dataAfter = target.getBytes();
        assertEq(dataAfter, bytes(hex"a1b1"));
    }

    function testZkCheatcodesCanMockCallTestContract() public {
        address thisAddress = address(this);
        MyProxyCaller transactor = new MyProxyCaller(thisAddress);

        vm.mockCall(thisAddress, abi.encodeWithSelector(IMyProxyCaller.transact.selector), abi.encode());

        transactor.transact();
    }

    function testZkCheatcodesCanMockCall(address mockMe) public {
        vm.assume(mockMe != address(vm));

        // zkVM currently doesn't support mocking the transaction sender
        vm.assume(mockMe != msg.sender);

        MyProxyCaller transactor = new MyProxyCaller(mockMe);

        vm.mockCall(mockMe, abi.encodeWithSelector(IMyProxyCaller.transact.selector), abi.encode());

        transactor.transact();
    }

    function testZkCheatcodesCanBeUsedAfterFork() public {
        assertEq(0, address(0x4e59b44847b379578588920cA78FbF26c0B4956C).balance);

        vm.createSelectFork(Globals.ETHEREUM_MAINNET_URL, ETH_FORK_BLOCK);
        assertEq(0, address(0x4e59b44847b379578588920cA78FbF26c0B4956C).balance);

        vm.deal(0x4e59b44847b379578588920cA78FbF26c0B4956C, 1 ether);
        assertEq(1 ether, address(0x4e59b44847b379578588920cA78FbF26c0B4956C).balance);
    }

    function testRecordLogsInZkVm() public {
        // ensure we are in zkvm
        vm.zkVm(true);
        vm.recordLogs();
        Emitter emitter = new Emitter(); // +7 logs from system contracts
        emitter.functionEmit(); // +3 from system contracts

        Vm.Log[] memory entries = vm.getRecordedLogs();
        assertEq(entries.length, 12);
        // 0,1: EthToken, 2,3: L1 Messanger, 4: Known Code Storage
        assertEq(entries[5].topics.length, 1);
        assertEq(entries[5].topics[0], keccak256("EventConstructor(string)"));
        assertEq(entries[5].data, abi.encode("constructor"));
        // 6: L2 Deployer, 7: EthToken

        // 8,9: EthToken
        assertEq(entries[10].topics.length, 1);
        assertEq(entries[10].topics[0], keccak256("EventFunction(string)"));
        assertEq(entries[10].data, abi.encode("function"));
        // 11: EthToken
    }

    function testRecordConsoleLogsLikeEVM() public {
        Emitter emitter = new Emitter();
        vm.makePersistent(address(emitter));

        // ensure we are in zkvm
        (bool _success, bytes memory _ret) = address(vm).call(abi.encodeWithSignature("zkVm(bool)", true));

        vm.recordLogs();
        emitter.emitConsole("zkvm");
        Vm.Log[] memory zkvmEntries = vm.getRecordedLogs();

        // ensure we are NOT in zkvm
        (_success, _ret) = address(vm).call(abi.encodeWithSignature("zkVm(bool)", false));

        vm.recordLogs();
        emitter.emitConsole("evm");
        Vm.Log[] memory evmEntries = vm.getRecordedLogs();

        assertEq(zkvmEntries.length, evmEntries.length);
    }

    // Utility function
    function getCodeCheck(string memory contractName, string memory outDir) internal {
        bytes memory bytecode = vm.getCode(contractName);

        string memory artifactPath = string.concat("zk/", outDir, "/", contractName, ".sol/", contractName, ".json");
        string memory artifact = vm.readFile(artifactPath);
        bytes memory expectedBytecode = vm.parseJsonBytes(artifact, ".bytecode.object");

        assertEq(bytecode, expectedBytecode, "code for the contract was incorrect");
    }
}

contract UsesCheatcodes {
    function getNonce(Vm vm, address target) public view returns (uint64) {
        return vm.getNonce(target);
    }

    function getZkBalance(Vm vm, address target) public view returns (uint256) {
        vm.zkVm(true);
        getNonce(vm, target);
        return target.balance;
    }
}

contract ZkCheatcodesInZkVmTest is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);
    UsesCheatcodes helper;

    function setUp() external {
        vm.zkVm(true);
        helper = new UsesCheatcodes();
        // ensure we can call cheatcodes from the helper
        vm.allowCheatcodes(address(helper));
        // and that the contract is kept between vm switches
        vm.makePersistent(address(helper));
    }

    function testCallVmInZkVm() external {
        address target = address(this);

        vm.expectRevert();
        helper.getNonce(vm, target);
    }

    function testCallVmAfterDisableZkVm() external {
        address target = address(this);
        uint64 expected = vm.getNonce(target);

        vm.zkVm(false);
        uint64 got = helper.getNonce(vm, target);

        assertEq(expected, got);
    }

    function testCallVmAfterDisableZkVmAndReEnable() external {
        address target = address(this);
        uint256 expected = target.balance;

        vm.zkVm(false);
        uint256 got = helper.getZkBalance(vm, target);

        assertEq(expected, got);
    }
}

contract Calculator {
    event Added(uint8 indexed sum);

    function add(uint8 a, uint8 b) public returns (uint8) {
        uint8 sum = a + b;
        emit Added(sum);
        return sum;
    }
}

contract EvmTargetContract is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);

    event Added(uint8 indexed sum);

    function exec() public {
        // We emit the event we expect to see.
        vm.expectEmit();
        emit Added(3);

        Calculator calc = new Calculator(); // deployed on zkEVM
        uint8 sum = calc.add(1, 2); // deployed on zkEVM
        assertEq(3, sum);
    }
}

contract ZkCheatcodeZkVmSkipTest is DSTest {
    Vm constant vm = Vm(HEVM_ADDRESS);
    EvmTargetContract helper;

    function setUp() external {
        vm.zkVm(true);
        helper = new EvmTargetContract();
        // ensure we can call cheatcodes from the helper
        vm.allowCheatcodes(address(helper));
        // and that the contract is kept between vm switches
        vm.makePersistent(address(helper));
    }

    function testFail_UseCheatcodesInZkVmWithoutSkip() external {
        helper.exec();
    }

    function testUseCheatcodesInEvmWithSkip() external {
        vm.zkVmSkip();
        helper.exec();
    }

    function testAutoSkipAfterDeployInEvmWithSkip() external {
        vm.zkVmSkip();
        EvmTargetContract helper2 = new EvmTargetContract();

        // this should auto execute in EVM
        helper2.exec();
    }
}
