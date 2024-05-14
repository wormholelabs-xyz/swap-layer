use crate::{
    composite::*,
    error::SwapLayerError,
    state::{StagedTransfer, StagedTransferInfo, StagedTransferSeeds},
};
use anchor_lang::prelude::*;
use anchor_spl::token;
use swap_layer_messages::messages::SwapMessageV1;
use swap_layer_messages::types::{OutputToken, RedeemMode};

#[derive(Accounts)]
pub struct CompleteTransferPayload<'info> {
    #[account(mut)]
    /// The payer of the transaction.
    payer: Signer<'info>,

    #[account(
        constraint = {
            let swap_msg = consume_swap_layer_fill.read_message_unchecked();

            require!(
                matches!(
                    swap_msg.output_token,
                    OutputToken::Usdc
                ),
                SwapLayerError::InvalidOutputToken
            );

            true
        }
    )]
    consume_swap_layer_fill: ConsumeSwapLayerFill<'info>,

    #[account(
        init_if_needed,
        payer = payer,
        space = try_compute_staged_transfer_size(&consume_swap_layer_fill.read_message_unchecked())?,
        seeds = [
            StagedTransfer::SEED_PREFIX,
            consume_swap_layer_fill.prepared_fill_key().as_ref(),
        ],
        bump
    )]
    staged_transfer: Account<'info, StagedTransfer>,

    #[account(
        init_if_needed,
        payer = payer,
        token::mint = usdc,
        token::authority = staged_transfer,
        seeds = [
            crate::STAGED_CUSTODY_TOKEN_SEED_PREFIX,
            staged_transfer.key().as_ref(),
        ],
        bump,
    )]
    staged_custody_token: Box<Account<'info, token::TokenAccount>>,

    usdc: Usdc<'info>,

    token_program: Program<'info, token::Token>,
    system_program: Program<'info, System>,
}

pub fn complete_transfer_payload(ctx: Context<CompleteTransferPayload>) -> Result<()> {
    // Consume the prepared fill, and send the tokens to the staged custody account.
    ctx.accounts.consume_swap_layer_fill.consume_prepared_fill(
        ctx.accounts.staged_custody_token.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
    )?;

    let swap_msg = ctx
        .accounts
        .consume_swap_layer_fill
        .read_message_unchecked();
    let staged_transfer = &mut ctx.accounts.staged_transfer;

    // Set the staged transfer if it hasn't been set yet.
    if staged_transfer.staged_by == Pubkey::default() {
        staged_transfer.set_inner(StagedTransfer {
            seeds: StagedTransferSeeds {
                prepared_fill: ctx.accounts.consume_swap_layer_fill.prepared_fill_key(),
                bump: ctx.bumps.staged_transfer,
            },
            info: StagedTransferInfo {
                staged_custody_token_bump: ctx.bumps.staged_custody_token,
                staged_by: ctx.accounts.payer.key(),
                source_chain: ctx.accounts.consume_swap_layer_fill.fill.source_chain,
                recipient: swap_msg.recipient,
                is_native: false,
            },
            recipient_payload: get_swap_message_payload(&swap_msg)?.to_vec(),
        });

        Ok(())
    } else {
        Ok(())
    }
}

fn try_compute_staged_transfer_size(swap_msg: &SwapMessageV1) -> Result<usize> {
    // Match on Payload redeem type.
    match &swap_msg.redeem_mode {
        RedeemMode::Payload(payload) => {
            let payload_size = payload.len();
            StagedTransfer::checked_compute_size(payload_size)
                .ok_or(error!(SwapLayerError::PayloadTooLarge))
        }
        _ => Err(SwapLayerError::InvalidRedeemMode.into()),
    }
}

fn get_swap_message_payload(swap_msg: &SwapMessageV1) -> Result<&[u8]> {
    match &swap_msg.redeem_mode {
        RedeemMode::Payload(payload) => Ok(payload),
        _ => Err(SwapLayerError::InvalidRedeemMode.into()),
    }
}
