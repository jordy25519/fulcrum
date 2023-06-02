// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.19;

import "forge-std/Test.sol";
import "forge-std/Vm.sol";
import "../lib/forge-std/src/console.sol";

import "../src/TradeExecutor.sol";

contract TradeExecutorTest is Test {
    TradeExecutor public executor;
    address constant payee = address(0x137);

    function setUp() public {
        executor = new TradeExecutor(payee);
    }

    function testDecode() public {
        uint128 payload = 0x000001f401f4ff0201000101;
        (uint8[3] memory exchanges, uint8[3] memory tokens, uint16[3] memory fees) = executor.decode(payload);

        uint256[3][3] memory tableTests = [
            [uint256(1), uint256(1), uint256(0)],
            [uint256(1), uint256(2), uint256(255)],
            [uint256(500), uint256(500), uint256(0)]
        ];
        for (uint8 i = 0; i < 3; i++) {
            assertEq(tableTests[0][i], uint256(exchanges[i]));
            assertEq(tableTests[1][i], uint256(tokens[i]));
            assertEq(tableTests[2][i], uint256(fees[i]));
        }
    }

    // 384,414 gas
    function testSwap2Step() public {
        // TODO: add encode side in solidity
        // univ3 usdc/weth 500 <> univ3 weth/usdc 3000
        uint128 payload = 0x00000bb801f4ff0100000000;
        uint128 amountIn = 10000 * 1e6;
        // USDC
        deal(0xFF970A61A04b1cA14834A43f5dE4533eBDDB5CC8, address(executor), amountIn);

        // expect 'failed successfully'
        vm.expectRevert(
            // abi.encodeWithSelector(TradeExecutor.Loss.selector, 1)
        );

        vm.prank(payee);
        executor.swap(amountIn, payload);
    }

    // 472,988 gas
    function testSwap3Step() public {
        // univ3 usdc/weth 500 <> univ3 weth/usdc 3000
        // sushi (weth, arb) -> uniswap (arb, usdc) -> camelot (usdc, weth)
        uint128 payload = 0x00000bb801f4000301010002;
        uint128 amountIn = 3 * 1e18;

        // weth
        deal(0x82aF49447D8a07e3bd95BD0d56f35241523fBab1, address(executor), amountIn);

        // 'failed successfully'
        vm.expectRevert(
            // abi.encodeWithSelector(TradeExecutor.Loss.selector, 1)
        );

        vm.prank(payee);
        executor.swap(amountIn, payload);
    }

    // 241,477 gas
    function testFlashSwap2StepUniV3Pools() public {
        // univ3 usdc/weth 500 <> univ3 weth/usdc 3000
        uint128 payload = 0x00000bb801f4ff0100000000;
        uint128 amountIn = 10000 * 1e6;

        vm.expectRevert(
            // abi.encodeWithSelector(TradeExecutor.Loss.selector, 1)
        );
        vm.prank(payee);

        executor.flashSwap(amountIn, payload);
    }

    // 235,690 gas
    function testFlashSwap2StepDifferentDex() public {
        // univ3 usdc/weth 500 <> univ3 weth/usdc 3000
        uint128 payload = 0x00000bb801f4ff0100000100;
        uint128 amountIn = 10000 * 1e6;

        vm.expectRevert(
            // abi.encodeWithSelector(TradeExecutor.Loss.selector, 1)
        );
        vm.prank(payee);

        executor.flashSwap(amountIn, payload);
    }

    // 348,667 gas
    function testFlashSwap3StepDifferentDex() public {
        // uniswap (weth, usdc) -> sushi (usdc, arb) -> uniswap (arb, weth)
        uint128 payload = 0x01f4000001f4030001000200;
        uint128 amountIn = 3 * 1e18;

        vm.expectRevert(
            // abi.encodeWithSelector(TradeExecutor.Loss.selector, 1)
        );
        vm.prank(payee);

        executor.flashSwap(amountIn, payload);
    }

    // gas for crossing uni v3 price ticks
    function testFlashSwapXTick() public {
        // uniswap (weth, usdc) -> sushi (usdc, arb) -> uniswap (arb, weth)
        uint128 payload = 0x01f4000001f4030001000200;
        uint128 amountIn = 1000 * 1e18;

        vm.expectRevert(
            // abi.encodeWithSelector(TradeExecutor.Loss.selector, 1)
        );
        vm.prank(payee);

        executor.flashSwap(amountIn, payload);
    }

    function testFlashSwapEntryAtV2Dex() public {
        // sushi (weth, usdc) -> uniswap (usdc, weth)
        uint128 payload = 0x01f401f401f4ff0001000002;
        uint128 amountIn = 5 * 1e18;

        vm.expectRevert(
            // abi.encodeWithSelector(TradeExecutor.Loss.selector, 1)
        );
        vm.prank(payee);

        executor.flashSwap(amountIn, payload);
    }

    function testFlashSwapEntryAtV2DexOneForZero() public {
        // sushi (weth, usdc) -> uniswap (usdc, weth)
        uint128 payload = 0x01f401f401f4ff0001000201;
        uint128 amountIn = 5 * 1e18;

        vm.expectRevert(
            // abi.encodeWithSelector(TradeExecutor.Loss.selector, 1)
        );
        vm.prank(payee);

        executor.flashSwap(amountIn, payload);
    }

    // 348,667 gas
    function testFlashSwap3StepOneForZero() public {
        // uniswap (usdc, weth) -> sushi (weth, arb) -> uniswap (arb, usdc)
        uint128 payload = 0x01f4000001f4030100000200;
        uint128 amountIn = 10000 * 1e6;

        vm.expectRevert(
            // abi.encodeWithSelector(TradeExecutor.Loss.selector, 1)
        );
        vm.prank(payee);

        executor.flashSwap(amountIn, payload);
    }

    function testWithdrawToken() public {
        deal(0x82aF49447D8a07e3bd95BD0d56f35241523fBab1, address(executor), 1e6);
        vm.prank(payee);
        executor.withdrawToken(0x82aF49447D8a07e3bd95BD0d56f35241523fBab1);
        assertEq(IERC20(0x82aF49447D8a07e3bd95BD0d56f35241523fBab1).balanceOf(payee), 1e6);

        vm.deal(address(executor), 1e18);
        vm.prank(payee);
        executor.withdrawNative();
        assertEq(payee.balance, 1e18);
    }

    function testAdminOnly() public {
        vm.expectRevert();
        executor.setPayee(address(0x157));
        vm.expectRevert();
        executor.setGateway(address(0x157));
        vm.expectRevert();
        executor.withdrawNative();
        vm.expectRevert();
        executor.withdrawToken(0x82aF49447D8a07e3bd95BD0d56f35241523fBab1);
    }

    function testSetAddresses() public {
        assertEq(executor.payee(), payee);
        assertEq(executor.gateway(), payee);

        vm.prank(payee);
        executor.setPayee(address(0x157));
        assertEq(executor.payee(), address(0x157));

        vm.prank(address(0x157));
        executor.setGateway(address(0x555));
        assertEq(executor.gateway(), address(0x555));
    }

    function testChronosDex() public {
        uint128 amountIn = uint128(0x0000000000000000000000000000000000000000000000000de0b6b3a7640000);
        uint128 payload = uint128(0x0000000000000000000000000000000000000000000000b401f4ff0401000300);
        vm.prank(payee);
        executor.flashSwap(amountIn, payload);
    }

    function testChronosDex2() public {
        uint128 amountIn = uint128(0x0000000000000000000000000000000000000000000000000de0b6b3a7640000);
        uint128 payload = uint128(0x000000000000000000000000000000000000000000b4006401f4000401030000);
        vm.prank(payee);
        executor.flashSwap(amountIn, payload);
    }
}