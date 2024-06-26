use crate::utils::gas_dropoff;
use crate::{composite::*, error::SwapLayerError};
use anchor_lang::prelude::*;
use anchor_spl::associated_token;
use swap_layer_messages::{messages::SwapMessageV1, types::RedeemMode};

#[derive(Accounts)]
pub struct CompleteSwapRelay<'info> {
    complete_swap: CompleteSwap<'info>,

    #[account(
        mut,
        address = associated_token::get_associated_token_address_with_program_id(
            &recipient.key(),
            &complete_swap.dst_mint.key(),
            &complete_swap.dst_token_program.key()
        )
    )]
    /// Recipient associated token account. The recipient authority check is necessary to ensure
    /// that the recipient is the intended recipient of the bridged tokens.
    ///
    /// If OutputToken::Other, this account will be deserialized to ensure that the recipient is
    /// the owner of this token account.
    ///
    /// CHECK: Mutable ATA whose owner is the recipient and mint is the destination mint.
    recipient_token: UncheckedAccount<'info>,

    /// CHECK: This account must be the owner of the recipient token account. The recipient token
    /// account must be encoded in the prepared fill.
    #[account(mut)]
    recipient: UncheckedAccount<'info>,
}

pub fn complete_swap_relay<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, CompleteSwapRelay<'info>>,
    instruction_data: Vec<u8>,
) -> Result<()>
where
    'c: 'info,
{
    let swap_msg = ctx.accounts.complete_swap.read_message_unchecked();

    match swap_msg.redeem_mode {
        RedeemMode::Relay {
            gas_dropoff,
            relaying_fee,
        } => handle_complete_swap_relay(
            ctx,
            instruction_data,
            swap_msg,
            gas_dropoff::denormalize_gas_dropoff(gas_dropoff),
            relaying_fee.into(),
        ),
        _ => err!(SwapLayerError::InvalidRedeemMode),
    }
}

pub fn handle_complete_swap_relay<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, CompleteSwapRelay<'info>>,
    instruction_data: Vec<u8>,
    swap_msg: SwapMessageV1,
    gas_dropoff: u64,
    relaying_fee: u64,
) -> Result<()>
where
    'c: 'info,
{
    // Consume prepared fill.
    let fill_amount = ctx.accounts.complete_swap.consume_prepared_fill()?;
    let payer = &ctx.accounts.complete_swap.payer;
    let recipient = &ctx.accounts.recipient;

    // Handle the relayer fee and gas dropoff. Override the relaying fee to zero
    // if the payer is the recipient (self redemption).
    let (in_amount, gas_dropoff) = if payer.key() != recipient.key() {
        (
            fill_amount
                .checked_sub(relaying_fee)
                .ok_or_else(|| SwapLayerError::InvalidRelayerFee)?,
            gas_dropoff.into(),
        )
    } else {
        (fill_amount, None)
    };

    complete_swap_jup_v6(
        &ctx.accounts.complete_swap,
        &ctx.bumps.complete_swap,
        ctx.remaining_accounts,
        instruction_data,
        in_amount,
        swap_msg,
        &ctx.accounts.recipient,
        &ctx.accounts.recipient_token,
        gas_dropoff,
    )
}
