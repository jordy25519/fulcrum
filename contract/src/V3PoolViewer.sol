// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import "./Buffer.sol";

interface IUniswapV2Pool {
    function getReserves() external view returns (uint128, uint128, uint256);
}

interface IUniswapV3Pool {
    function liquidity() external view returns (uint128);
    function slot0()
        external
        view
        returns (
            uint160 sqrtPriceX96,
            int24 tick,
            uint16 observationIndex,
            uint16 observationCardinality,
            uint16 observationCardinalityNext,
            uint8 feeProtocol,
            bool unlocked
        );
}

contract V3PoolViewer {
    using Buffer for Buffer.buffer;

    // Query the given UniswapV3 pools for price and liquidity fields
    // @dev input and return data is tightly packed
    function getPoolData(bytes calldata v3Pools, bytes calldata v2Pools)
        public
        view
        returns (bytes memory v3PoolData, bytes memory v2PoolData)
    {
        if (v3Pools.length > 0) {
            v3PoolData = getPriceAndLiquidityV3(v3Pools);
        }
        if (v2Pools.length > 0) {
            v2PoolData = getReservesV2(v2Pools);
        }

        return (v3PoolData, v2PoolData);
    }

    // Query the given UniswapV2 style pools for reserves
    function getReservesV2(bytes calldata pools) public view returns (bytes memory results) {
        uint256 poolCount = pools.length / 20;
        Buffer.buffer memory buf;
        Buffer.init(buf, poolCount * 32);

        for (uint256 i = 0; i < poolCount; ++i) {
            address pool = bytesToAddress(pools[i * 20:(i + 1) * 20]);
            (uint128 reserve0, uint128 reserve1,) = IUniswapV2Pool(pool).getReserves();
            buf.appendBytes16(bytes16(reserve0));
            buf.appendBytes16(bytes16(reserve1));
        }

        return buf.buf;
    }

    // Query the given UniswapV3 pools for price and liquidity fields
    // @dev input and return data is tightly packed
    function getPriceAndLiquidityV3(bytes calldata pools) public view returns (bytes memory results) {
        uint256 poolCount = pools.length / 20;
        Buffer.buffer memory buf;
        Buffer.init(buf, poolCount * 36);

        for (uint256 i = 0; i < poolCount; ++i) {
            address pool = bytesToAddress(pools[i * 20:(i + 1) * 20]);
            (uint160 sqrtPX96,,,,,,) = IUniswapV3Pool(pool).slot0();
            buf.appendBytes20(bytes20(sqrtPX96));
            buf.appendBytes16(bytes16(IUniswapV3Pool(pool).liquidity()));
        }

        return buf.buf;
    }

    /// Query a single UniswapV3 pool for its price and liquidity fields
    function getPriceAndLiquidityV3Single(address pool) public view returns (uint160 sqrtPX96, uint128 liquidity) {
        (sqrtPX96,,,,,,) = IUniswapV3Pool(pool).slot0();
        liquidity = IUniswapV3Pool(pool).liquidity();

        return (sqrtPX96, liquidity);
    }

    function bytesToAddress(bytes calldata data) private pure returns (address addr) {
        bytes memory b = data;
        assembly {
            addr := mload(add(b, 20))
        }
    }
}
