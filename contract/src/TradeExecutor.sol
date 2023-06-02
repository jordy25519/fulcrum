// SPDX-License-Identifier: MIT
pragma solidity =0.8.19;
pragma abicoder v2;

import "./External.sol";
import "../lib/forge-std/src/console.sol";

/// Prioritizes a lightweight ABI to make offchain component as fast as possible vs. gas cost savings
contract TradeExecutor is IUniswapV3SwapCallback {
    // uniV3 constants
    uint160 internal constant MIN_SQRT_RATIO = 4295128739;
    uint160 internal constant MAX_SQRT_RATIO = 1461446703485210103287273052203988822378723970342;
    // where to send profits
    address public payee;
    // gateway contract
    address public gateway;
    address[] public tokenLookup;

    error Loss(uint);

    address private constant UNISWAP_V3_ROUTER = 0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45;
    address private constant CAMELOT_V2_ROUTER = 0xc873fEcbd354f5A56E00E710B90EF4201db2448d;
    address private constant SUSHI_ROUTER = 0x1b02dA8Cb0d097eB8D57A175b88c7D8b47997506;
    address private constant CHRONOS_ROUTER = 0xE708aA9E887980750C040a6A2Cb901c37Aa34f3b;

    address private constant CHRONOS_FACTORY = 0x7C7b7dE557282411358575f3322e321c94245A9F;
    address private constant SUSHI_FACTORY = 0xc35DADB65012eC5796536bD9864eD8773aBc74C4;
    address private constant CAMELOT_V2_FACTORY = 0x6EcCab422D763aC031210895C81787E87B43A652;
    address private constant UNISWAP_V3_FACTORY = 0x1F98431c8aD98523631AE4a59f267346ea31F984;

    uint8 private constant UNISWAP_V3_ID = 0;
    uint8 private constant CAMELOT_ID = 1;
    uint8 private constant SUSHI_ID = 2;
    uint8 private constant CHRONOS_ID = 3;

    event SetLookup(uint8 id, address token);

    constructor(address _payee) {
        payee = _payee;
        gateway = _payee;
        // init token id to address mapping
        // the indexes are expected to be 1:1 with the fulcrum client `Token` enum
        tokenLookup = [
            0xFF970A61A04b1cA14834A43f5dE4533eBDDB5CC8, // usdc
            0x82aF49447D8a07e3bd95BD0d56f35241523fBab1, // weth
            0x2f2a2543B76A4166549F7aaB2e75Bef0aefC5B0f, // wbtc
            0x912CE59144191C1204E64559FE8253a0e49E6548, // arb
            0xFd086bC7CD5C481DCC9C85ebE478A1C0b69FCbb9, // usdt
            0xDA10009cBd5D07dd0CeCc66161FC93D7c9000da1 // dai
        ];

        uint256 approvalLimit = type(uint128).max;
        IERC20(0xFF970A61A04b1cA14834A43f5dE4533eBDDB5CC8).approve(
            UNISWAP_V3_ROUTER, approvalLimit
        );
        IERC20(0x912CE59144191C1204E64559FE8253a0e49E6548).approve(
            UNISWAP_V3_ROUTER, approvalLimit
        );
        IERC20(0x82aF49447D8a07e3bd95BD0d56f35241523fBab1).approve(
            UNISWAP_V3_ROUTER, approvalLimit
        );
        IERC20(0xFd086bC7CD5C481DCC9C85ebE478A1C0b69FCbb9).approve(
            UNISWAP_V3_ROUTER, approvalLimit
        );

        IERC20(0xFF970A61A04b1cA14834A43f5dE4533eBDDB5CC8).approve(
            CAMELOT_V2_ROUTER, approvalLimit
        );
        IERC20(0x912CE59144191C1204E64559FE8253a0e49E6548).approve(
            CAMELOT_V2_ROUTER, approvalLimit
        );
        IERC20(0x82aF49447D8a07e3bd95BD0d56f35241523fBab1).approve(
            CAMELOT_V2_ROUTER, approvalLimit
        );

        IERC20(0xFF970A61A04b1cA14834A43f5dE4533eBDDB5CC8).approve(
            SUSHI_ROUTER, approvalLimit
        );
        IERC20(0x82aF49447D8a07e3bd95BD0d56f35241523fBab1).approve(
            SUSHI_ROUTER, approvalLimit
        );

        IERC20(0xFF970A61A04b1cA14834A43f5dE4533eBDDB5CC8).approve(
            CHRONOS_ROUTER, approvalLimit
        );
        IERC20(0x912CE59144191C1204E64559FE8253a0e49E6548).approve(
            CHRONOS_ROUTER, approvalLimit
        );
        IERC20(0x82aF49447D8a07e3bd95BD0d56f35241523fBab1).approve(
            CHRONOS_ROUTER, approvalLimit
        );
        IERC20(0xFd086bC7CD5C481DCC9C85ebE478A1C0b69FCbb9).approve(
            CHRONOS_ROUTER, approvalLimit
        );
    }

    receive() external payable {}

    /// decodes fulcrum trade data
    function decode(uint128 payload)
        public
        pure
        returns (uint8[3] memory exchanges, uint8[3] memory tokens, uint16[3] memory fees)
    {
        uint8 exchange0Id = uint8(payload);
        uint8 exchange1Id = uint8(payload >> 8);
        uint8 exchange2Id = uint8(payload >> 16);
        uint8 token0Id = uint8(payload >> 24);
        uint8 token1Id = uint8(payload >> 32);
        uint8 token2Id = uint8(payload >> 40);
        uint16 fee0 = uint16(payload >> 48);
        uint16 fee1 = uint16(payload >> 64);
        uint16 fee2 = uint16(payload >> 80);

        return ([exchange0Id, exchange1Id, exchange2Id], [token0Id, token1Id, token2Id], [fee0, fee1, fee2]);
    }

    // delegate approval for 'who' to spend tokens from this contract e.g. to a router contract
    function setPayee(address who) external {
        require(msg.sender == payee);
        payee = who;
    }

    function setGateway(address who) external {
        require(msg.sender == payee);
        gateway = who;
    }

    // delegate approval for 'who' to spend tokens from this contract e.g. to a router contract
    function setApproval(address who, address token, uint256 amount) external {
        require(msg.sender == payee);
        IERC20(token).approve(who, amount);
    }

    // set the token address associated with `id`
    function setTokenId(uint8 id, address token) external {
        require(msg.sender == payee);
        tokenLookup[id] = token;
        emit SetLookup(id, token);
    }

    // Withdraw erc20 token from the contract
    function withdrawToken(address token) external {
        require(msg.sender == payee);
        IERC20(token).transfer(payee, IERC20(token).balanceOf(address(this)));
    }

    // Withdraw native token from the contract
    function withdrawNative() external {
        require(msg.sender == payee);
        payable(msg.sender).transfer(address(this).balance);
    }

    // @dev execute a flash swap across up to 3 exchanges and 3 pools
    // the ABI is an attempt at optimizing for tx creation & transmission speed
    // this will revert if the final position is not in profit
    function swap(uint128 amountIn, uint128 payload) external {
        require(msg.sender == gateway);
        (uint8[3] memory exchanges, uint8[3] memory tokens, uint16[3] memory fees) = decode(payload);
        address token0 = tokenLookup[tokens[0]];
        address token1 = tokenLookup[tokens[1]];

        uint256 amountOut = swapExactIn(exchanges[0], amountIn, token0, token1, fees[0]);
        if (tokens[2] >= tokenLookup.length) {
            // 2 step
            amountOut = swapExactIn(exchanges[1], amountOut, token1, token0, fees[1]);
        } else {
            // triangle
            address token2 = tokenLookup[tokens[2]];
            amountOut = swapExactIn(exchanges[1], amountOut, token1, token2, fees[1]);
            amountOut = swapExactIn(exchanges[2], amountOut, token2, token0, fees[2]);
        }

        if (amountOut < amountIn) revert Loss(amountIn - amountOut);
        IERC20(token0).transfer(payee, amountOut - amountIn);
    }

    // Execute a flash swap across up to 3 exchanges and 3 pools
    // @dev the ABI is an attempt at optimizing for tx creation & transmission speed
    function flashSwap(uint128 amountIn, uint128 payload) external {
        require(msg.sender == gateway);
        (uint8[3] memory exchanges, uint8[3] memory tokens, uint16[3] memory fees) = decode(payload);
        address token0 = tokenLookup[tokens[0]];
        address token1 = tokenLookup[tokens[1]];
        uint8 exchangeId0 = exchanges[0];
    
        console.logUint(amountIn);

        if (exchangeId0 == UNISWAP_V3_ID) {
            IUniswapV3Pool pool0 = IUniswapV3Pool(
                PoolAddress.computeAddress(UNISWAP_V3_FACTORY, PoolAddress.getPoolKey(token0, token1, fees[0]))
            );
            // convert payload to bytes for callback abi
            bytes memory callbackData = new bytes(32);
            assembly {
                mstore(add(callbackData, 32), payload)
            }
            if (token0 < token1) {
                // pool0.flash(address(this), amountIn, 0, callbackData);
                pool0.swap(address(this), true, int128(amountIn), MIN_SQRT_RATIO + 1, callbackData);
            } else {
                // pool0.flash(address(this), 0, amountIn, callbackData);
                pool0.swap(address(this), false, int128(amountIn), MAX_SQRT_RATIO - 1, callbackData);
            }
        } else {
            // stack too deep
            {
                address pool0;
                if (exchangeId0 == CHRONOS_ID) {
                    pool0 = IChronosFactory(CHRONOS_FACTORY).getPair(token0, token1, false);
                } else if (exchangeId0 == CAMELOT_ID) {
                    pool0 = IUniswapV2Factory(CAMELOT_V2_FACTORY).getPair(token0, token1);
                } else if (exchangeId0 == SUSHI_ID) {
                    pool0 = IUniswapV2Factory(SUSHI_FACTORY).getPair(token0, token1);
                }
                (uint256 reserve0, uint256 reserve1,) = IUniswapV2Pair(pool0).getReserves();

                // if we are given a trade usdc/weth -> weth/usdc
                // cannot borrow usdc then swap from the same pool...
                // but we know there's a profitable trade so we skip the first trade, borrow the output then trade and
                // return different token by swapping back to the pool
                // lock modifier prevents retrading on this pool...
                bytes memory callbackData = abi.encode(payload, amountIn);
                // a,b b,a
                // pull b, must paybcak
                if (token0 < token1) {
                    uint256 amountOut = UniswapV2Math.getAmountOut(uint256(amountIn), reserve0, reserve1);
                    IUniswapV2Pair(pool0).swap(0, amountOut, address(this), callbackData);
                } else {
                    uint256 amountOut = UniswapV2Math.getAmountOut(uint256(amountIn), reserve1, reserve0);
                    IUniswapV2Pair(pool0).swap(amountOut, 0, address(this), callbackData);
                }
            }
        }
    }

    function uniswapV2Call(address, uint256 amount0, uint256 amount1, bytes calldata data) external {
        // we have entered a flash position, now finish the arb
        (uint128 payload, uint128 amountInOwed) = abi.decode(data, (uint128, uint128));

        (uint8[3] memory exchanges, uint8[3] memory tokens, uint16[3] memory fees) = decode(payload);
        address token0 = tokenLookup[tokens[0]];
        address token1 = tokenLookup[tokens[1]];
        uint8 token2Id = tokens[2];

        uint256 loanAmountOut = amount0 > 0 ? amount0 : amount1;
        uint256 netAmountOut = loanAmountOut;
        // TODO: const
        if (token2Id == 255) {
            // 2 step
            // we are holding token 1 at this point
            netAmountOut = swapExactIn(exchanges[1], netAmountOut, token1, token0, fees[1]);
        } else {
            // triangle
            address token2 = tokenLookup[token2Id];
            netAmountOut = swapExactIn(exchanges[1], netAmountOut, token1, token2, fees[1]);
            netAmountOut = swapExactIn(exchanges[2], netAmountOut, token2, token0, fees[2]);
        }

        // always payback in the starting token(0)
        payback(amountInOwed, netAmountOut, token0);
    }

    function uniswapV3SwapCallback(int256 amount0Delta, int256 amount1Delta, bytes calldata data) external {
        // we have entered a flash position, now finish the arb
        uint128 payload = uint128(uint256(bytes32(data)));
        (uint8[3] memory exchanges, uint8[3] memory tokens, uint16[3] memory fees) = decode(payload);

        // scoping to fix stack depth
        address token0 = tokenLookup[tokens[0]];
        address token1 = tokenLookup[tokens[1]];
        uint8 token2Id = tokens[2];

        // run trades
        uint256 amountInputOwed;
        uint256 amountInputEarned;

        // swap is loaning the output amount/token  (need to payback input by the end)
        // by this point we are paid in token1 by the initial swap
        if (token0 < token1) {
            // is zeroForOne
            amountInputOwed = uint256(amount0Delta);
            amountInputEarned = uint256(-amount1Delta);
        } else {
            amountInputOwed = uint256(amount1Delta);
            amountInputEarned = uint256(-amount0Delta);
        }
        console.logUint(amountInputEarned);
        // TODO: const
        if (token2Id == 255) {
            // 2 step
            // (uint8 exchangeId, uint amountIn, address tokenIn, address tokenOut, uint16 fee) private returns (uint) {
            amountInputEarned = swapExactIn(exchanges[1], amountInputEarned, token1, token0, fees[1]);
            console.logUint(amountInputEarned);
        } else {
            address token2 = tokenLookup[token2Id];
            // triangle
            amountInputEarned = swapExactIn(exchanges[1], amountInputEarned, token1, token2, fees[1]);
            console.logUint(amountInputEarned);
            amountInputEarned = swapExactIn(exchanges[2], amountInputEarned, token2, token0, fees[2]);
            console.logUint(amountInputEarned);
        }
        payback(amountInputOwed, amountInputEarned, token0);
    }

    function swapExactIn(uint8 exchangeId, uint256 amountIn, address tokenIn, address tokenOut, uint16 fee)
        private
        returns (uint256 amountOut)
    {
        address recipient = address(this);
        if (exchangeId == UNISWAP_V3_ID) {
            bool zeroForOne = tokenIn < tokenOut;
            uint160 sqrtPriceLimitX96 = zeroForOne ? MIN_SQRT_RATIO + 1 : MAX_SQRT_RATIO - 1;
            ISwapRouter.ExactInputSingleParams memory params = ISwapRouter.ExactInputSingleParams({
                tokenIn: tokenIn,
                tokenOut: tokenOut,
                fee: fee,
                recipient: recipient,
                amountIn: amountIn,
                amountOutMinimum: 0,
                sqrtPriceLimitX96: sqrtPriceLimitX96
            });
            ISwapRouter uniswapV3Router = ISwapRouter(UNISWAP_V3_ROUTER);
            return uniswapV3Router.exactInputSingle(params);
        }

        if (exchangeId == CAMELOT_ID) {
            address[] memory path = new address[](2);
            path[0] = tokenIn;
            path[1] = tokenOut;
            ICamelotRouter camelotRouter = ICamelotRouter(CAMELOT_V2_ROUTER);
            uint256 beforeBalance = IERC20(tokenOut).balanceOf(address(this));
            // euck, this interface doesn't return amountOut
            camelotRouter.swapExactTokensForTokensSupportingFeeOnTransferTokens(
                amountIn,
                0,
                path,
                recipient,
                address(0), // referrer
                block.timestamp
            );
            return IERC20(tokenOut).balanceOf(address(this)) - beforeBalance;
        } else if (exchangeId == CHRONOS_ID) {
            IChronosRouter router = IChronosRouter(CHRONOS_ROUTER);
            uint256[] memory amounts = router.swapExactTokensForTokensSimple(amountIn, 0, tokenIn, tokenOut, false, recipient, block.timestamp);
            // amounts [[in][out]]
            return amounts[1];
        } else if (exchangeId == SUSHI_ID) {
            address[] memory path = new address[](2);
            path[0] = tokenIn;
            path[1] = tokenOut;
            // uniswap v2 ABI
            IUniswapV2Router02 router = IUniswapV2Router02(SUSHI_ROUTER);
            uint256[] memory amounts = router.swapExactTokensForTokens(amountIn, 0, path, recipient, block.timestamp);
            // amounts [[in][out]]
            return amounts[1];
        }
    }

    // payback the flash swap loan and send profits to 'payee'
    function payback(uint256 loanAmount, uint256 earnedAmount, address loanToken) private {
        if (earnedAmount <= loanAmount) revert Loss(loanAmount - earnedAmount);

        IERC20(loanToken).transfer(msg.sender, loanAmount);
        IERC20(loanToken).transfer(payee, earnedAmount - loanAmount);
    }
}
