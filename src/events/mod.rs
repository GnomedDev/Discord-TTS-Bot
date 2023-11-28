#![allow(clippy::module_name_repetitions)]

mod channel;
mod guild;
mod member;
mod message;
mod other;
mod ready;
mod voice_state;

use channel::*;
use guild::*;
use member::*;
use message::*;
use other::*;
use ready::*;
use voice_state::*;

use poise::serenity_prelude as serenity;

use crate::structs::{FrameworkContext, Result, SerenityContext};

pub async fn listen(
    ctx: &SerenityContext,
    event: &serenity::FullEvent,
    fw_ctx: FrameworkContext<'_>,
) -> Result<()> {
    match event {
        serenity::FullEvent::Message { new_message } => message(ctx, new_message, fw_ctx).await,
        serenity::FullEvent::GuildCreate { guild, is_new } => {
            guild_create(ctx, guild, *is_new).await
        }
        serenity::FullEvent::Ready { data_about_bot } => ready(ctx, fw_ctx, data_about_bot).await,
        serenity::FullEvent::GuildDelete { incomplete, full } => {
            guild_delete(ctx, incomplete, full.as_ref()).await
        }
        serenity::FullEvent::GuildMemberAddition { new_member } => {
            guild_member_addition(ctx, new_member).await
        }
        serenity::FullEvent::GuildMemberRemoval { guild_id, user, .. } => {
            guild_member_removal(ctx, *guild_id, user).await
        }
        serenity::FullEvent::VoiceStateUpdate { old, new } => {
            voice_state_update(ctx, old.as_ref(), new).await
        }
        serenity::FullEvent::ChannelDelete { channel, .. } => {
            channel_delete(&ctx.data, channel).await
        }
        serenity::FullEvent::InteractionCreate { interaction } => {
            interaction_create(ctx, interaction).await
        }
        serenity::FullEvent::Resume { .. } => {
            resume(&ctx.data);
            Ok(())
        }
        _ => Ok(()),
    }
}
