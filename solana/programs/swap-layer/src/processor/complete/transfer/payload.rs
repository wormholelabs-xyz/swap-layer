use crate::{
    composite::*,
    error::SwapLayerError,
    state::{StagedInbound, StagedInboundInfo, StagedInboundSeeds},
};
use anchor_lang::prelude::*;
use anchor_spl::token;
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
        space = StagedInbound::try_compute_size_if_needed(
            staged_inbound,
            consume_swap_layer_fill.read_message_unchecked()
        )?,
        seeds = [
            StagedInbound::SEED_PREFIX,
            consume_swap_layer_fill.prepared_fill_key().as_ref(),
        ],
        bump
    )]
    staged_inbound: Account<'info, StagedInbound>,

    #[account(
        init_if_needed,
        payer = payer,
        token::mint = usdc,
        token::authority = staged_inbound,
        seeds = [
            crate::STAGED_CUSTODY_TOKEN_SEED_PREFIX,
            staged_inbound.key().as_ref(),
        ],
        bump,
    )]
    staged_custody_token: Box<Account<'info, token::TokenAccount>>,

    usdc: Usdc<'info>,

    token_program: Program<'info, token::Token>,
    system_program: Program<'info, System>,
}

pub fn complete_transfer_payload(ctx: Context<CompleteTransferPayload>) -> Result<()> {
    let staged_inbound = &mut ctx.accounts.staged_inbound;

    // Set the staged transfer if it hasn't been set yet.
    if staged_inbound.uninitialized() {
        // Consume the prepared fill, and send the tokens to the staged custody account.
        ctx.accounts.consume_swap_layer_fill.consume_prepared_fill(
            ctx.accounts.staged_custody_token.as_ref().as_ref(),
            &ctx.accounts.token_program,
        )?;

        let swap_msg = ctx
            .accounts
            .consume_swap_layer_fill
            .read_message_unchecked();

        match swap_msg.redeem_mode {
            RedeemMode::Payload { sender, buf } => staged_inbound.set_inner(StagedInbound {
                seeds: StagedInboundSeeds {
                    prepared_fill: ctx.accounts.consume_swap_layer_fill.prepared_fill_key(),
                    bump: ctx.bumps.staged_inbound,
                },
                info: StagedInboundInfo {
                    custody_token: ctx.accounts.staged_custody_token.key(),
                    staged_by: ctx.accounts.payer.key(),
                    source_chain: ctx.accounts.consume_swap_layer_fill.fill.source_chain,
                    sender,
                    recipient: Pubkey::from(swap_msg.recipient),
                    is_native: false,
                },
                recipient_payload: buf.into(),
            }),
            _ => return err!(SwapLayerError::InvalidRedeemMode),
        };
    }

    Ok(())
}
