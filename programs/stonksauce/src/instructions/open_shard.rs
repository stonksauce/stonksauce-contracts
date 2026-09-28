use crate::error::SauceError;
use crate::state::MAX_SHARDS;
use crate::OpenShard;
use anchor_lang::prelude::*;

pub fn handler(ctx: Context<OpenShard>, index: u32) -> Result<()> {
    let mut directory = ctx.accounts.directory.load_mut()?;
    require!(index == directory.shard_count, SauceError::ShardOutOfOrder);
    require!((index as usize) < MAX_SHARDS, SauceError::TooManyShards);

    let round = ctx.accounts.jar.round_index;
    let mut shard = ctx.accounts.shard.load_init()?;
    shard.jar = ctx.accounts.jar.key();
    shard.index = index;
    shard.next_slot = 1;
    shard.round = round;

    directory.shard_count = index + 1;
    directory.rounds[index as usize] = round;
    Ok(())
}
