// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.19;

import "forge-std/Test.sol";
import "forge-std/Vm.sol";
import "../lib/forge-std/src/console.sol";

import "../src/V3PoolViewer.sol";

contract ExecutorTest is Test {
    V3PoolViewer public viewer;

    address sushiUsdcWeth = 0x905dfCD5649217c42684f23958568e533C711Aa3;
    address chronosUsdcWeth = 0xA2F1C1B52E1b7223825552343297Dc68a29ABecC;

    function packAddresses(address[] memory addresses) private pure returns(bytes memory data){
        for(uint i=0; i < addresses.length; i++){
            data = abi.encodePacked(data, addresses[i]);
        }
    }

    function testViewer() public {
        viewer = new V3PoolViewer();

        // build v3 pools query
        address[] memory v3Addresses = new address[](2);
        v3Addresses[0] = 0xC31E54c7a869B9FcBEcc14363CF510d1c41fa443;
        v3Addresses[1] = 0xcDa53B1F66614552F834cEeF361A8D12a0B8DaD8;
        bytes memory v3Pools = packAddresses(v3Addresses);

        // build v2 pools query
        address[] memory v2Addresses = new address[](2);
        v2Addresses[0] = sushiUsdcWeth;
        v2Addresses[1] = chronosUsdcWeth;
        bytes memory v2Pools = packAddresses(v2Addresses);

        // test
        (bytes memory v3PoolData, bytes memory v2PoolData) = viewer.getPoolData(v3Pools, v2Pools);

        // test v3 pools output
        (uint160 p1, uint128 l1) = viewer.getPriceAndLiquidityV3Single(0xC31E54c7a869B9FcBEcc14363CF510d1c41fa443);
        (uint160 p2, uint128 l2) = viewer.getPriceAndLiquidityV3Single(0xcDa53B1F66614552F834cEeF361A8D12a0B8DaD8);
        assert(p1 > 0);
        assert(l1 > 0);
        uint160[2] memory price = [p1, p2];
        uint128[2] memory liquidity = [l1, l2];

        this.assertDecodedPoolDataV3(v3PoolData, price, liquidity);

        // test v2 pools output
        (uint128 r0a, uint128 r1a,) = IUniswapV2Pool(sushiUsdcWeth).getReserves();
        (uint128 r0b, uint128 r1b,) = IUniswapV2Pool(chronosUsdcWeth).getReserves();
        uint128[2] memory reserves0 = [r0a, r0b];
        uint128[2] memory reserves1 = [r1a, r1b];

        this.assertDecodedPoolDataV2(v2PoolData, reserves0, reserves1);
    }

    // https://ethereum.stackexchange.com/questions/103437/converting-bytes-memory-to-bytes-calldata
    function assertDecodedPoolDataV3(bytes calldata data, uint160[2] calldata price, uint128[2] calldata liquidity) public pure {
        for(uint i; i < 2; i++) {
            uint offset = i * 36;
            uint160 p = uint160(bytes20(data[offset: offset + 20]));
            uint128 l = uint128(bytes16(data[offset + 20: offset + 36]));
            assert(price[i] == p);
            assert(liquidity[i] == l);
        }
    }

    function assertDecodedPoolDataV2(bytes calldata data, uint128[2] calldata reserves0, uint128[2] calldata reserves1) public pure {
        for(uint i; i < 2; i++) {
            uint offset = i * 32;
            uint128 r0 = uint128(bytes16(data[offset: offset + 16]));
            uint128 r1 = uint128(bytes16(data[offset + 16: offset + 32]));
            assert(reserves0[i] == r0);
            assert(reserves1[i] == r1);
        }
    }
}

