use crate::utils::{JupiterV6, RoutePlanStep};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SwapJupiterV6SharedAccountsRoutePassthrough<'info> {
    jupiter_v6_program: Program<'info, JupiterV6>,
}

#[derive(Debug, AnchorSerialize, AnchorDeserialize, Clone)]
pub struct SwapJupiterV6SharedAccountsRoutePassthroughArgs {
    id: u8,
    route_plan: Vec<RoutePlanStep>,
    in_amount: u64,
    quoted_out_amount: u64,
    slippage_bps: u16,
    platform_fee_bps: u8,
}

pub fn swap_jupiter_v6_shared_accounts_route_passthrough(
    _ctx: Context<SwapJupiterV6SharedAccountsRoutePassthrough>,
    selector: [u8; 8],
    args: SwapJupiterV6SharedAccountsRoutePassthroughArgs,
) -> Result<()> {
    msg!("Selector: {:?}", selector);
    msg!("Args: {:?}", args);
    Ok(())
}
