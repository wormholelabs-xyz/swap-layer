// SPDX-License-Identifier: Apache 2

pragma solidity ^0.8.24;

import "./assets/SwapLayerSansRouterImpls.sol";
import "./assets/SwapLayerUniswapUR.sol";
import "./assets/SwapLayerTraderJoe.sol";

contract SwapLayer is SwapLayerSansRouterImpls
  //comment in/out inheritance and functions below to enable/disable support for a given router
  , SwapLayerUniswapUR
  , SwapLayerTraderJoe
  {
  //constructor of the logic contract setting immutables
  constructor(
    address liquidityLayer,
    address permit2,
    address wnative,
    address uniswapRouter,
    address traderJoeRouter
  )
  SwapLayerSansRouterImpls(
    liquidityLayer,
    permit2,
    wnative,
    uniswapRouter,
    traderJoeRouter
  ) {}

  //uncomment and comment out inheritance to disable support for a given router
  // error NotSupported();
  //
  // function _uniswapMaxApprove(IERC20 token) internal override pure {}
  // function _uniswapSwap(bool, uint, uint, IERC20, IERC20, bool, bool, bytes memory)
  //   internal override pure returns (uint) { revert NotSupported(); }
  //
  // function _traderJoeMaxApprove(IERC20 token) internal override pure {}
  // function _traderJoeSwap(bool, uint, uint, IERC20, IERC20, bool, bool, bytes memory)
  //   internal override pure returns (uint) { revert NotSupported(); }
}
