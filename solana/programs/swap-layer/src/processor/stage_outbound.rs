use crate::{
    composite::*,
    error::SwapLayerError,
    state::{Peer, RedeemOption, StagedOutbound, StagedOutboundInfo, StagedRedeem},
    utils, TRANSFER_AUTHORITY_SEED_PREFIX,
};
use anchor_lang::{prelude::*, system_program};
use anchor_spl::token;
use common::wormhole_io::{Readable, Writeable};
use solana_program::keccak;
use swap_layer_messages::types::OutputToken;

#[derive(Accounts)]
#[instruction(args: StageOutboundArgs)]
pub struct StageOutbound<'info> {
    #[account(mut)]
    payer: Signer<'info>,

    /// This signer is mutable in case the integrator wants to separate the payer of accounts from
    /// the sender, who may be sending lamports ([StageOutboundArgs::is_native] is true).
    #[account(mut)]
    sender: Option<Signer<'info>>,

    #[account(
        seeds = [
            TRANSFER_AUTHORITY_SEED_PREFIX,
            &keccak::hash(&args.try_to_vec()?).0,
        ],
        bump,
        constraint = sender_token.is_some() @ SwapLayerError::SenderTokenRequired,
    )]
    program_transfer_authority: Option<UncheckedAccount<'info>>,

    /// If provided, this token account's mint must be equal to the source mint.
    ///
    /// NOTE: This account may not be necessary because the sender may send lamports directly
    /// ([StageOutboundArgs::is_native] is true).
    #[account(
        mut,
        token::mint = src_mint,
    )]
    sender_token: Option<Account<'info, token::TokenAccount>>,

    /// Peer used to determine whether assets are sent to a valid destination. The registered peer
    /// will also act as the authority over the staged custody token account.
    ///
    /// Ordinarily we could consider the authority to be the staged outbound account itself. But
    /// because this account can be signed for outside of this program (either keypair or PDA), the
    /// token account would then be out of this program's control.
    #[account(
        constraint = {
            require_eq!(
                args.target_chain,
                target_peer.seeds.chain,
                SwapLayerError::InvalidTargetChain,
            );

            true
        }
    )]
    target_peer: RegisteredPeer<'info>,

    /// Staged outbound account, which contains all of the instructions needed to initiate a
    /// transfer on behalf of the sender.
    #[account(
        init,
        payer = payer,
        space = StagedOutbound::try_compute_size(&args.redeem_option, &args.encoded_output_token)?,
        constraint = {
            // Cannot send to zero address.
            require!(args.recipient != [0; 32], SwapLayerError::InvalidRecipient);

            true
        }
    )]
    staged_outbound: Account<'info, StagedOutbound>,

    /// Custody token account for the staged outbound transfer. This account will be owned by the
    /// registered peer.
    #[account(
        init,
        payer = payer,
        token::mint = src_mint,
        token::authority = target_peer,
        seeds = [
            crate::STAGED_CUSTODY_TOKEN_SEED_PREFIX,
            staged_outbound.key().as_ref(),
        ],
        bump,
    )]
    staged_custody_token: Account<'info, token::TokenAccount>,

    #[account(
        mut,
        token::mint = common::USDC_MINT,
    )]
    usdc_refund_token: Box<Account<'info, token::TokenAccount>>,

    /// Mint can either be USDC or whichever mint is used to swap into USDC.
    src_mint: Account<'info, token::Mint>,

    token_program: Program<'info, token::Token>,
    system_program: Program<'info, System>,
}

/// Arguments for [stage_outbound].
#[derive(Debug, Clone, AnchorSerialize, AnchorDeserialize)]
pub struct StageOutboundArgs {
    pub amount_in: u64,

    /// The Wormhole chain ID of the network to transfer tokens to.
    pub target_chain: u16,

    /// The recipient of the transfer.
    pub recipient: [u8; 32],

    pub redeem_option: Option<RedeemOption>,

    pub encoded_output_token: Option<Vec<u8>>,
}

pub fn stage_outbound(ctx: Context<StageOutbound>, args: StageOutboundArgs) -> Result<()> {
    // In case we use a program transfer authority, we need to use these for the transfer.
    let last_transfer_authority_signer_seeds = ctx
        .bumps
        .program_transfer_authority
        .map(|bump| (keccak::hash(&args.try_to_vec().unwrap()).0, bump));

    let StageOutboundArgs {
        amount_in,
        target_chain,
        recipient,
        redeem_option,
        encoded_output_token,
    } = args;

    // Replace None with OutputToken::USDC encoded.
    let encoded_output_token = encoded_output_token.unwrap_or({
        let mut buf = Vec::with_capacity(1);
        OutputToken::Usdc.write(&mut buf).unwrap();
        buf
    });
    let output_token = OutputToken::read(&mut &encoded_output_token[..]).unwrap();

    // We need to determine the relayer fee. This fee will either be paid for right now if
    // StagedInput::Usdc or will be paid for later if a swap is required to get USDC.
    let (transfer_amount, staged_redeem) = match redeem_option {
        Some(redeem_option) => match redeem_option {
            RedeemOption::Relay {
                gas_dropoff,
                max_relayer_fee,
            } => {
                // Relaying fee must be less than the user-specific maximum.
                let relaying_fee = utils::relayer_fees::calculate_relayer_fee(
                    &ctx.accounts.target_peer.relay_params,
                    gas_dropoff,
                    &output_token,
                )?;
                require!(
                    relaying_fee <= max_relayer_fee,
                    SwapLayerError::ExceedsMaxRelayingFee
                );

                (
                    if ctx.accounts.src_mint.key() == common::USDC_MINT {
                        relaying_fee
                            .checked_add(amount_in)
                            .ok_or(SwapLayerError::U64Overflow)?
                    } else {
                        amount_in
                    },
                    StagedRedeem::Relay {
                        gas_dropoff,
                        relaying_fee,
                    },
                )
            }
            RedeemOption::Payload(buf) => (amount_in, StagedRedeem::Payload(buf)),
        },
        None => (amount_in, StagedRedeem::Direct),
    };

    let token_program = &ctx.accounts.token_program;
    let custody_token = &ctx.accounts.staged_custody_token;

    let sender = match &ctx.accounts.sender_token {
        Some(sender_token) => match (
            &ctx.accounts.sender,
            &ctx.accounts.program_transfer_authority,
        ) {
            (Some(sender), None) => {
                token::transfer(
                    CpiContext::new(
                        token_program.to_account_info(),
                        token::Transfer {
                            from: sender_token.to_account_info(),
                            to: custody_token.to_account_info(),
                            authority: sender.to_account_info(),
                        },
                    ),
                    transfer_amount,
                )?;

                sender.key()
            }
            (None, Some(program_transfer_authority)) => {
                let (hashed_args, authority_bump) = last_transfer_authority_signer_seeds.unwrap();

                token::transfer(
                    CpiContext::new_with_signer(
                        ctx.accounts.token_program.to_account_info(),
                        token::Transfer {
                            from: sender_token.to_account_info(),
                            to: custody_token.to_account_info(),
                            authority: program_transfer_authority.to_account_info(),
                        },
                        &[&[
                            crate::TRANSFER_AUTHORITY_SEED_PREFIX,
                            &hashed_args,
                            &[authority_bump],
                        ]],
                    ),
                    transfer_amount,
                )?;

                sender_token.owner
            }
            _ => return err!(SwapLayerError::EitherSenderOrProgramTransferAuthority),
        },
        None => match &ctx.accounts.sender {
            Some(sender) => {
                system_program::transfer(
                    CpiContext::new(
                        ctx.accounts.system_program.to_account_info(),
                        system_program::Transfer {
                            from: sender.to_account_info(),
                            to: custody_token.to_account_info(),
                        },
                    ),
                    transfer_amount,
                )?;

                let peer_seeds = &ctx.accounts.target_peer.seeds;
                token::sync_native(CpiContext::new_with_signer(
                    token_program.to_account_info(),
                    token::SyncNative {
                        account: custody_token.to_account_info(),
                    },
                    &[&[
                        Peer::SEED_PREFIX,
                        &peer_seeds.chain.to_be_bytes(),
                        &[peer_seeds.bump],
                    ]],
                ))?;

                sender.key()
            }
            None => return err!(SwapLayerError::SenderRequired),
        },
    };

    ctx.accounts.staged_outbound.set_inner(StagedOutbound {
        info: StagedOutboundInfo {
            custody_token_bump: ctx.bumps.staged_custody_token,
            prepared_by: ctx.accounts.payer.key(),
            usdc_refund_token: ctx.accounts.usdc_refund_token.key(),
            sender,
            target_chain,
            recipient,
        },
        staged_redeem,
        encoded_output_token,
    });

    // Done.
    Ok(())
}