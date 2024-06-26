// SPDX-License-Identifier: Apache 2

pragma solidity ^0.8.24;

import { BytesParsing } from "wormhole-sdk/libraries/BytesParsing.sol";

import "./SwapLayerBase.sol";

uint8 constant UNIVERSAL_ROUTER_EXACT_IN = 0;
uint8 constant UNIVERSAL_ROUTER_EXACT_OUT = 1;

interface IUniversalRouter {
  //inputs = abi.encode(
  //  address recipient,
  //  uint256 inOutAmount,
  //  uint256 limitAmount,
  //  bytes   path,
  //  boolean payerIsSender - always true
  //)
  function execute(
    bytes calldata commands,
    bytes[] calldata inputs
  ) external payable;
}

abstract contract SwapLayerUniswapUR is SwapLayerBase {
  function _uniswapMaxApprove(IERC20 token) internal override {
    if (_uniswapRouter == address(0))
      return;

    _maxApprove(token, address(_permit2));
    _permit2.approve(address(token), _uniswapRouter, type(uint160).max, type(uint48).max);
  }

  function _uniswapSwap(
    bool isExactIn,
    uint inputAmount,
    uint outputAmount,
    IERC20 inputToken,
    IERC20 outputToken,
    bool revertOnFailure,
    bool approveCheck,
    bytes memory path
  ) internal override returns (uint /*inOutAmount*/) { unchecked {
    if (approveCheck) {
      //universal router always uses permit2 for transfers...
      //see here: https://github.com/Uniswap/universal-router/blob/41183d6eb154f0ab0e74a0e911a5ef9ea51fc4bd/contracts/modules/uniswap/v3/V3SwapRouter.sol#L65
      //and here: https://github.com/Uniswap/universal-router/blob/41183d6eb154f0ab0e74a0e911a5ef9ea51fc4bd/contracts/modules/Permit2Payments.sol#L41
      (uint allowance,, ) =
        _permit2.allowance(address(this), address(inputToken), _uniswapRouter);
      if (allowance < inputAmount)
        _uniswapMaxApprove(inputToken);
    }

    if (isExactIn) {
      (uint balanceBefore, uint balanceAfter) = _universalRouterSwap(
        UNIVERSAL_ROUTER_EXACT_IN,
        outputToken,
        inputAmount,
        outputAmount,
        path,
        revertOnFailure
      );
      return balanceAfter - balanceBefore;
    }
    else {
      {
        //TODO either eventually replace this with proper memcpy or adjust parseEvmSwapParams
        //     so it expects an inverse path order for exact out swaps in case of uniswap and
        //     also composes the usdc and outputToken path correctly (switch on isExactIn)
        uint size = 20;
        uint offset = path.length - size;
        (bytes memory invertedPath, ) = BytesParsing.sliceUnchecked(path, offset, size);
        do {
          size = size == 20 ? 3 : 20;
          offset -= size;
          bytes memory slice;
          (slice, ) = BytesParsing.sliceUnchecked(path, offset, size);
          invertedPath = abi.encodePacked(invertedPath, slice);
        } while (offset != 0);
        path = invertedPath;
      }

      (uint balanceBefore, uint balanceAfter) = _universalRouterSwap(
        UNIVERSAL_ROUTER_EXACT_OUT,
        inputToken,
        outputAmount,
        inputAmount,
        path,
        revertOnFailure
      );
      return balanceBefore - balanceAfter;
    }
  }}

  function _universalRouterSwap(
    uint8 command,
    IERC20 unknownBalanceToken,
    uint256 inOutAmount,
    uint256 limitAmount,
    bytes memory path,
    bool revertOnFailure
  ) private returns (uint balanceBefore, uint balanceAfter) {
    bytes[] memory inputs = new bytes[](1);
    inputs[0] = abi.encode(address(this), inOutAmount, limitAmount, path, true);
    bytes memory funcCall =
      abi.encodeCall(IUniversalRouter.execute, (abi.encodePacked(command), inputs));

    balanceBefore = unknownBalanceToken.balanceOf(address(this));
    (bool success, bytes memory result) = _uniswapRouter.call(funcCall);
    if (!success) {
      if (revertOnFailure)
        revert SwapFailed(result);
      else
        balanceBefore = 0; //return (0,0)
    }
    else
      balanceAfter = unknownBalanceToken.balanceOf(address(this));
  }
}