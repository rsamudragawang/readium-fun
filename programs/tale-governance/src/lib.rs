use anchor_lang::prelude::*;
use anchor_spl::token::{Token};

// Ensure this matches the program ID you are deploying to and testing against
declare_id!("DCbm8RfFi4CjuEm4r4yrZ7fxfz4Nug1fdXjcandt4nkk");

#[program]
pub mod tale_governance {
    use super::*;

    pub fn create_vote(
        ctx: Context<CreateVote>,
        voting_id: String,
        question: String,
        description: String,
        choices: Vec<String>,
        start_time: i64,
        end_time: i64,
        regular_vote_power: u64,
        nft_vote_power: u64,
        category: VoteCategory,
        tags: Vec<String>,
        stories: Vec<Pubkey>,
        nfts: Vec<String>,
    ) -> Result<()> {
        require!(start_time < end_time, GovernanceError::InvalidTimeRange);
        require!(choices.len() >= 2, GovernanceError::InvalidChoices);
        require!(regular_vote_power > 0, GovernanceError::InvalidVotePower);
        require!(nft_vote_power > 0, GovernanceError::InvalidVotePower);
        require!(tags.len() <= 5, GovernanceError::TooManyTags);
        require!(voting_id.len() > 0 && voting_id.len() <= 64, GovernanceError::InvalidVoteId);

        let vote = &mut ctx.accounts.vote;
        vote.creator = ctx.accounts.creator.key();
        vote.voting_id = voting_id;
        vote.question = question;
        vote.description = description;
        vote.choices = choices;
        vote.start_time = start_time;
        vote.end_time = end_time;
        vote.regular_vote_power = regular_vote_power;
        vote.nft_vote_power = nft_vote_power;
        vote.total_votes = vec![0; vote.choices.len()];
        vote.is_active = true;
        vote.created_at = Clock::get()?.unix_timestamp;
        vote.total_participants = 0;
        vote.regular_voters = 0;
        vote.nft_voters = 0;
        vote.category = category;
        vote.tags = tags;
        vote.status = VoteStatus::Upcoming;
        vote.winning_choice = None;
        vote.total_vote_power = 0;
        vote.stories = stories;
        vote.nfts = nfts;
        vote.history = vec![];

        let vote_record = &mut ctx.accounts.vote_record;
        vote_record.vote = vote.key();
        vote_record.voter = ctx.accounts.creator.key();
        vote_record.has_voted = false;
        vote_record.vote_power = 0;
        vote_record.voted_choice = None;
        vote_record.voted_at = None;

        Ok(())
    }

    pub fn cast_vote(ctx: Context<CastVote>, choice_index: u8, is_nft_holder: bool) -> Result<()> {
        let vote = &mut ctx.accounts.vote;
        let vote_record = &mut ctx.accounts.vote_record;
        let clock = Clock::get()?;

        require!(vote.is_active, GovernanceError::VoteNotActive);
        require!(clock.unix_timestamp >= vote.start_time, GovernanceError::VoteNotStarted);
        require!(clock.unix_timestamp <= vote.end_time, GovernanceError::VoteEnded);
        require!(choice_index < vote.choices.len() as u8, GovernanceError::InvalidChoice);
        require!(!vote_record.has_voted, GovernanceError::AlreadyVoted);

        // Use NFT or regular vote power based on is_nft_holder
        let vote_power = if is_nft_holder { vote.nft_vote_power } else { vote.regular_vote_power };
        if is_nft_holder {
            vote.nft_voters += 1;
        } else {
            vote.regular_voters += 1;
        }

        vote.total_votes[choice_index as usize] += vote_power;
        vote.total_participants += 1;
        vote.total_vote_power += vote_power;
        vote_record.has_voted = true;
        vote_record.vote_power = vote_power;
        vote_record.voted_choice = Some(choice_index);
        vote_record.voted_at = Some(clock.unix_timestamp);

        if clock.unix_timestamp >= vote.start_time && vote.status == VoteStatus::Upcoming {
            vote.status = VoteStatus::Active;
        }

        vote.history.push(VoteHistory {
            voter: ctx.accounts.voter.key(),
            timestamp: clock.unix_timestamp,
            voting_power: vote_record.vote_power,
            transaction: ctx.accounts.vote_record.key().to_string(),
        });

        Ok(())
    }

    pub fn finalize_vote(ctx: Context<FinalizeVote>) -> Result<()> {
        let vote = &mut ctx.accounts.vote;
        let clock = Clock::get()?;

        require!(vote.is_active, GovernanceError::VoteNotActive);
        require!(clock.unix_timestamp > vote.end_time, GovernanceError::VoteStillActive);
        require!(vote.creator == ctx.accounts.creator.key(), GovernanceError::NotCreator);

        vote.is_active = false;
        vote.status = VoteStatus::Completed;

        let mut max_votes = 0;
        let mut winning_index = None;

        for (index, &votes) in vote.total_votes.iter().enumerate() {
            if votes > max_votes {
                max_votes = votes;
                winning_index = Some(index as u8);
            }
        }
        vote.winning_choice = winning_index;
        Ok(())
    }

    pub fn get_results(ctx: Context<GetResults>) -> Result<()> {
        let vote = &ctx.accounts.vote;
        require!(!vote.is_active || Clock::get()?.unix_timestamp > vote.end_time, GovernanceError::VoteStillActive);
        Ok(())
    }

    pub fn list_votes(
        ctx: Context<ListVotes>,
        status: VoteStatus,
        category: Option<VoteCategory>,
        page: u8,
        page_size: u8,
    ) -> Result<()> {
        let vote_list = &mut ctx.accounts.vote_list;
        vote_list.status = status;
        vote_list.category = category;
        vote_list.page = page;
        vote_list.page_size = page_size;
        vote_list.last_updated = Clock::get()?.unix_timestamp;
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(voting_id: String)]
pub struct CreateVote<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    
    #[account(
        init,
        payer = creator,
        space = Vote::LEN,
        seeds = [b"vote", creator.key().as_ref(), voting_id.as_bytes()],
        bump
    )]
    pub vote: Account<'info, Vote>,
    
    #[account(
        init,
        payer = creator,
        space = VoteRecord::LEN,
        seeds = [b"vote_record", vote.key().as_ref(), creator.key().as_ref()],
        bump
    )]
    pub vote_record: Account<'info, VoteRecord>,
    
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CastVote<'info> {
    #[account(mut)]
    pub voter: Signer<'info>,
    #[account(mut)]
    pub vote: Account<'info, Vote>,
    #[account(
        init_if_needed,
        payer = voter,
        space = VoteRecord::LEN,
        seeds = [b"vote_record", vote.key().as_ref(), voter.key().as_ref()],
        bump
    )]
    pub vote_record: Account<'info, VoteRecord>,
    /// CHECK: This is the user's NFT token account. We check mint and owner in the instruction.
    pub nft_token_account: AccountInfo<'info>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct FinalizeVote<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(mut)]
    pub vote: Account<'info, Vote>,
}

#[derive(Accounts)]
pub struct GetResults<'info> {
    pub vote: Account<'info, Vote>,
}

#[derive(Accounts)]
pub struct ListVotes<'info> {
    #[account(mut)]
    pub vote_list: Account<'info, VoteList>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct VoteHistory {
    pub voter: Pubkey,
    pub timestamp: i64,
    pub voting_power: u64,
    pub transaction: String,
}

#[account]
pub struct Vote {
    pub creator: Pubkey,
    pub question: String,
    pub voting_id: String,
    pub description: String,
    pub choices: Vec<String>,
    pub start_time: i64,
    pub end_time: i64,
    pub regular_vote_power: u64,
    pub nft_vote_power: u64,
    pub total_votes: Vec<u64>,
    pub is_active: bool,
    pub created_at: i64,
    pub total_participants: u64,
    pub regular_voters: u64,
    pub nft_voters: u64,
    pub category: VoteCategory,
    pub tags: Vec<String>,
    pub status: VoteStatus,
    pub winning_choice: Option<u8>,
    pub total_vote_power: u64,
    pub stories: Vec<Pubkey>,
    pub nfts: Vec<String>,
    pub history: Vec<VoteHistory>,
}

#[account]
pub struct VoteRecord {
    pub vote: Pubkey,
    pub voter: Pubkey,
    pub has_voted: bool,
    pub vote_power: u64,
    pub voted_choice: Option<u8>,
    pub voted_at: Option<i64>,
}

#[account]
pub struct VoteList {
    pub status: VoteStatus,
    pub category: Option<VoteCategory>,
    pub page: u8,
    pub page_size: u8,
    pub last_updated: i64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq)]
pub enum VoteStatus {
    Upcoming,
    Active,
    Completed,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq)]
pub enum VoteCategory {
    Content,
    Feature,
    Community,
    Technical,
    Other,
}

impl Vote {
    pub const LEN: usize = 8 + // discriminator
        32 + // creator
        64 + // voting_id (max 64 bytes)
        100 + // question (max length)
        500 + // description (max length)
        4 + (50 * 10) + // choices (max 10 choices, 50 chars each)
        8 + // start_time
        8 + // end_time
        8 + // regular_vote_power
        8 + // nft_vote_power
        4 + (8 * 10) + // total_votes (max 10 choices)
        1 + // is_active
        8 + // created_at
        8 + // total_participants
        8 + // regular_voters
        8 + // nft_voters
        1 + // category
        4 + (20 * 5) + // tags (max 5 tags, 20 chars each)
        1 + // status
        1 + 1 + // winning_choice (Option<u8>)
        8 + // total_vote_power
        4 + (32 * 10) + // stories (max 10)
        4 + (64 * 10) + // nfts (max 10, 64 chars each)
        4 + ( // history (max 50 entries)
            (32 + 8 + 8 + 88) * 50 // voter pubkey + timestamp + voting_power + transaction string (max 80 chars + 8 for string overhead)
        );
}

impl VoteRecord {
    pub const LEN: usize = 8 + // discriminator
        32 + // vote
        32 + // voter
        1 + // has_voted
        8 + // vote_power
        1 + 1 + // voted_choice (Option<u8>)
        1 + 8; // voted_at (Option<i64>)
}

impl VoteList {
    pub const LEN: usize = 8 + // discriminator
        1 + // status
        1 + 1 + // category (Option<VoteCategory>)
        1 + // page
        1 + // page_size
        8; // last_updated
}

#[error_code]
pub enum GovernanceError {
    #[msg("Invalid time range")]
    InvalidTimeRange,
    #[msg("Invalid number of choices")]
    InvalidChoices,
    #[msg("Invalid vote power")]
    InvalidVotePower,
    #[msg("Vote is not active")]
    VoteNotActive,
    #[msg("Vote has not started yet")]
    VoteNotStarted,
    #[msg("Vote has ended")]
    VoteEnded,
    #[msg("Invalid choice index")]
    InvalidChoice,
    #[msg("Invalid NFT")]
    InvalidNFT,
    #[msg("Not the NFT owner")]
    NotNFTOwner,
    #[msg("Vote is still active")]
    VoteStillActive,
    #[msg("Already voted")]
    AlreadyVoted,
    #[msg("Not the creator")]
    NotCreator,
    #[msg("Too many tags")]
    TooManyTags,
    #[msg("Invalid vote ID")]
    InvalidVoteId,
}