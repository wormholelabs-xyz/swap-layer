use crate::{composite::*, error::SwapLayerError};
use anchor_lang::prelude::*;
use anchor_spl::{associated_token, token, token_interface};
use swap_layer_messages::{
    messages::SwapMessageV1,
    types::{JupiterV6SwapParameters, OutputSwap, OutputToken, RedeemMode, SwapType},
};

#[derive(Accounts)]
pub struct CompleteSwapDirect<'info> {
    complete_swap: CompleteSwap<'info>,

    #[account(
        mut,
        address = associated_token::get_associated_token_address(
            &recipient.key(),
            &complete_swap.dst_mint.key()
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

pub fn complete_swap_direct<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, CompleteSwapDirect<'info>>,
    instruction_data: Vec<u8>,
) -> Result<()>
where
    'c: 'info,
{
    let SwapMessageV1 {
        recipient,
        redeem_mode,
        output_token,
    } = ctx.accounts.complete_swap.read_message_unchecked();

    require_keys_eq!(
        ctx.accounts.recipient.key(),
        Pubkey::from(recipient),
        SwapLayerError::InvalidRecipient
    );

    match redeem_mode {
        RedeemMode::Direct => match output_token {
            OutputToken::Usdc => {
                // In this case, we require that the signer of the instruction (the payer) is the
                // recipient himself.
                require_keys_eq!(
                    ctx.accounts.complete_swap.payer.key(),
                    ctx.accounts.recipient.key(),
                    SwapLayerError::InvalidRecipient
                );

                handle_complete_swap_direct_jup_v6(
                    ctx,
                    instruction_data,
                    Default::default(),
                    Default::default(),
                )
            }
            OutputToken::Gas(OutputSwap {
                deadline: _,
                limit_amount,
                swap_type: SwapType::JupiterV6(swap_params),
            }) => handle_complete_swap_direct_jup_v6(
                ctx,
                instruction_data,
                (limit_amount.try_into().unwrap(), swap_params).into(),
                true,
            ),
            OutputToken::Other {
                address: _,
                swap:
                    OutputSwap {
                        deadline: _,
                        limit_amount,
                        swap_type: SwapType::JupiterV6(swap_params),
                    },
            } => handle_complete_swap_direct_jup_v6(
                ctx,
                instruction_data,
                (limit_amount.try_into().unwrap(), swap_params).into(),
                false,
            ),
            _ => err!(SwapLayerError::InvalidOutputToken),
        },
        _ => err!(SwapLayerError::InvalidRedeemMode),
    }
}

pub fn handle_complete_swap_direct_jup_v6<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, CompleteSwapDirect<'info>>,
    ix_data: Vec<u8>,
    limit_and_params: Option<(u64, JupiterV6SwapParameters)>,
    is_native: bool,
) -> Result<()>
where
    'c: 'info,
{
    // Consume prepared fill.
    let in_amount = ctx.accounts.complete_swap.consume_prepared_fill()?;

    let swap_authority = &ctx.accounts.complete_swap.authority;

    let prepared_fill_key = &ctx.accounts.complete_swap.prepared_fill_key();
    let swap_authority_seeds = &[
        crate::SWAP_AUTHORITY_SEED_PREFIX,
        prepared_fill_key.as_ref(),
        &[ctx.bumps.complete_swap.authority],
    ];

    let (shared_accounts_route, mut swap_args, cpi_remaining_accounts) =
        JupiterV6SharedAccountsRoute::set_up(ctx.remaining_accounts, &ix_data[..])?;

    // Verify remaining accounts.
    {
        require_keys_eq!(
            shared_accounts_route.transfer_authority.key(),
            swap_authority.key(),
            SwapLayerError::InvalidSwapAuthority
        );
        require_keys_eq!(
            shared_accounts_route.src_custody_token.key(),
            ctx.accounts.complete_swap.src_swap_token.key(),
            SwapLayerError::InvalidSourceSwapToken
        );
        require_keys_eq!(
            shared_accounts_route.dst_custody_token.key(),
            ctx.accounts.complete_swap.dst_swap_token.key(),
            SwapLayerError::InvalidDestinationSwapToken
        );
        require_keys_eq!(
            shared_accounts_route.src_mint.key(),
            common::USDC_MINT,
            SwapLayerError::InvalidSourceMint
        );
        require_keys_eq!(
            shared_accounts_route.dst_mint.key(),
            ctx.accounts.complete_swap.dst_mint.key(),
            SwapLayerError::InvalidDestinationMint
        );
    }

    let limit_amount = match limit_and_params {
        // If the limit amount is some value (meaning that the OutputToken is Gas or Other), we
        // will override the instruction arguments with the limit amount and slippage == 0 bps.
        // Otherwise we will compute the limit amount using the given swap args.
        Some((limit_amount, swap_params)) => {
            msg!(
                "Override in_amount: {}, quoted_out_amount: {}, slippage_bps: {}",
                swap_args.in_amount,
                swap_args.quoted_out_amount,
                swap_args.slippage_bps
            );
            swap_args.in_amount = in_amount;
            swap_args.quoted_out_amount = limit_amount;
            swap_args.slippage_bps = 0;

            // Peek into the head of remaining accounts. This account will be the dex program that Jupiter
            // V6 interacts with. If the swap params specify a specific dex program, we need to ensure that
            // the one passed into this instruction handler is that.
            if let Some(dex_program_id) = swap_params.dex_program_id {
                require_eq!(
                    swap_args.route_plan.len(),
                    1,
                    SwapLayerError::NotJupiterV6DirectRoute
                );
                require_keys_eq!(
                    cpi_remaining_accounts[0].key(),
                    Pubkey::from(dex_program_id),
                    SwapLayerError::JupiterV6DexProgramMismatch
                );
            }

            limit_amount.into()
        }
        None => {
            // Fetched swap args should have the same in amount as the prepared (fast) fill.
            require_eq!(
                swap_args.in_amount,
                in_amount,
                SwapLayerError::InvalidSwapInAmount
            );

            None
        }
    };

    // Execute swap.
    let amount_out = shared_accounts_route.swap_exact_in(
        swap_args,
        swap_authority_seeds,
        cpi_remaining_accounts,
        limit_amount,
    )?;

    let recipient = &ctx.accounts.recipient;

    // We perform a token transfer if the output token is not gas.
    if !is_native {
        // Verify that the encoded owner is the actual owner. ATAs are no different from other token
        // accounts, so anyone can set the authority of an ATA to be someone else.
        {
            let mut acc_data: &[_] = &ctx.accounts.recipient_token.data.borrow();
            let recipient_token_owner =
                token::TokenAccount::try_deserialize(&mut acc_data).map(|token| token.owner)?;
            require_keys_eq!(
                recipient_token_owner,
                recipient.key(),
                ErrorCode::ConstraintTokenOwner,
            );
        }

        let dst_mint = &ctx.accounts.complete_swap.dst_mint;

        // Transfer destination tokens to recipient.
        token_interface::transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts
                    .complete_swap
                    .dst_token_program
                    .to_account_info(),
                token_interface::TransferChecked {
                    from: ctx.accounts.complete_swap.dst_swap_token.to_account_info(),
                    to: ctx.accounts.recipient_token.to_account_info(),
                    authority: swap_authority.to_account_info(),
                    mint: dst_mint.to_account_info(),
                },
                &[swap_authority_seeds],
            ),
            amount_out,
            dst_mint.decimals,
        )?;
    }

    // NOTE: If the output token is gas, lamports reflecting the WSOL amount will be transferred to
    // the recipient's account.
    ctx.accounts
        .complete_swap
        .close_swap_accounts(&ctx.bumps.complete_swap, recipient)
}
