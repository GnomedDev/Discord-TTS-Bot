// Discord TTS Bot
// Copyright (C) 2021-Present David Thomas

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
use sysinfo::{SystemExt, ProcessExt};
use poise::serenity_prelude as serenity;
use num_format::{Locale, ToFormattedString};

use crate::constants::{OPTION_SEPERATORS};
use crate::structs::{Context, TTSMode, OptionTryUnwrap, CommandResult};
use crate::funcs::{fetch_audio, netural_colour, parse_user_or_guild, refresh_kind};

/// Shows how long TTS Bot has been online
#[poise::command(category="Extra Commands", prefix_command, slash_command, required_bot_permissions="SEND_MESSAGES")]
pub async fn uptime(ctx: Context<'_>,) -> CommandResult {
    ctx.say(format!(
        "<@{}> has been up since: <t:{}:R>", 
        ctx.discord().cache.current_user_id(),
        ctx.data().start_time.duration_since(std::time::UNIX_EPOCH)?.as_secs()
    )).await?;

    Ok(())
}

/// Generates TTS and sends it in the current text channel!
#[poise::command(category="Extra Commands", prefix_command, slash_command, required_bot_permissions="SEND_MESSAGES | ATTACH_FILES")]
pub async fn tts(
    ctx: Context<'_>, 
    #[description="The text to TTS"] #[rest] message: String
) -> CommandResult {
    let data = ctx.data();
    let author = ctx.author();

    if let poise::Context::Prefix(_) = ctx {
        if let Some(guild) = ctx.guild() {
            let author_voice_state = guild.voice_states.get(&author.id);
            let bot_voice_state = guild.voice_states.get(&ctx.discord().cache.current_user_id());
            if let (Some(bot_voice_state), Some(author_voice_state)) = (bot_voice_state, author_voice_state) {
                if bot_voice_state.channel_id == author_voice_state.channel_id {
                    let setup_channel: i64 = data.guilds_db.get(guild.id.into()).await?.get("channel");
                    if setup_channel as u64 == ctx.channel_id().0 {
                        ctx.say(format!("You don't need to include the `{}tts` for messages to be said!", ctx.prefix())).await?;
                        return Ok(())
                    }
                }
            }
        }
    }

    let attachment = {
        let (voice, mode) = parse_user_or_guild(data, author.id, ctx.guild_id()).await?;

        let author_name: String = author.name.chars().filter(|char| {char.is_alphanumeric()}).collect();
        let speaking_rate = data.userinfo_db.get(author.id.into()).await?.get("speaking_rate");

        serenity::AttachmentType::Bytes {
            data: std::borrow::Cow::Owned(fetch_audio(
                &data.reqwest, &data.config.tts_service,
                message, &voice, mode, speaking_rate
            ).await?),
            filename: format!("{}-{}.{}", author_name, ctx.id(), match mode {
                TTSMode::gTTS => "mp3",
                TTSMode::eSpeak => "wav",
                TTSMode::Premium => "ogg"
            })
        }
    };

    ctx.defer().await?;
    ctx.send(|b| {b
        .content("Generated some TTS!")
        .attachment(attachment)
    }).await?;

    Ok(())
}

/// Shows various different stats
#[poise::command(category="Extra Commands", prefix_command, slash_command, required_bot_permissions="SEND_MESSAGES | EMBED_LINKS")]
pub async fn botstats(ctx: Context<'_>,) -> CommandResult {
    ctx.defer_or_broadcast().await?;

    let data = ctx.data();
    let ctx_discord = ctx.discord();
    let bot_user_id = ctx_discord.cache.current_user_id();

    let start_time = std::time::SystemTime::now();

    let guilds: Vec<serenity::Guild> = ctx_discord.cache.guilds().iter()
        .filter_map(|g| g.to_guild_cached(ctx_discord))
        .collect();

    let total_members = guilds.iter().map(|g| g.member_count).sum::<u64>().to_formatted_string(&Locale::en);
    let total_voice_clients = guilds.iter().filter_map(|g| g.voice_states.get(&bot_user_id)).count();
    let total_guild_count = guilds.len();

    let shard_count = ctx_discord.cache.shard_count();
    let ram_usage = {
        let mut system_info = data.system_info.lock();
        system_info.refresh_specifics(refresh_kind());

        let pid = sysinfo::get_current_pid().unwrap();
        system_info.process(pid).unwrap().memory() / 1024
    };

    let [sep1, sep2, ..] = OPTION_SEPERATORS;
    let netural_colour = netural_colour(crate::premium_check(ctx_discord, data, ctx.guild_id()).await?.is_none());

    let time_to_fetch = start_time.elapsed()?.as_secs_f64() * 1000.0;
    ctx.send(|b| {b.embed(|e| { e
        .title(format!("{}: Freshly rewritten in Rust!", ctx_discord.cache.current_user_field(|u| u.name.clone())))
        .thumbnail(ctx_discord.cache.current_user_field(serenity::CurrentUser::face))
        .url(&data.config.main_server_invite)
        .colour(netural_colour)
        .footer(|f| {f
            .text(format!("
Time to fetch: {time_to_fetch:.2}ms
Support Server: https://discord.gg/zWPWwQC
Repository: https://github.com/GnomedDev/Discord-TTS-Bot
            ", ))
        })
        .description(format!("
Currently in:
    {sep2} {total_voice_clients} voice channels
    {sep2} {total_guild_count} servers
Currently using:
    {sep1} {shard_count} shards
    {sep1} {ram_usage:.1}MB of RAM
and can be used by {total_members} people!"))
    })}).await?;

    Ok(())
}

/// Shows the current setup channel!
#[poise::command(category="Extra Commands", guild_only, prefix_command, slash_command, required_bot_permissions="SEND_MESSAGES")]
pub async fn channel(ctx: Context<'_>,) -> CommandResult {
    let channel: i64 = ctx.data().guilds_db.get(ctx.guild_id().unwrap().into()).await?.get("channel");

    if channel as u64 == ctx.channel_id().0 {
        ctx.say("You are in the setup channel already!").await?;
    } else if channel != 0 {
        ctx.say(format!("The current setup channel is: <#{channel}>")).await?;
    } else {
        ctx.say(format!("The channel hasn't been setup, do `{}setup #textchannel`", ctx.prefix())).await?;
    }

    Ok(())
}

/// Shows how you can help support TTS Bot's development and hosting!
#[poise::command(category="Extra Commands", prefix_command, slash_command, required_bot_permissions="SEND_MESSAGES", aliases("purchase", "premium"))]
pub async fn donate(ctx: Context<'_>,) -> CommandResult {
    ctx.say(format!("
To donate to support the development and hosting of {} and get access to TTS Bot Premium, a more stable version of this bot \
with more and better voices you can donate via Patreon!\nhttps://www.patreon.com/Gnome_the_Bot_Maker
    ", ctx.discord().cache.current_user_field(|u| u.name.clone()))).await?;

    Ok(())
}

/// Gets current ping to discord!
#[poise::command(category="Extra Commands", prefix_command, slash_command, required_bot_permissions="SEND_MESSAGES", aliases("lag"))]
pub async fn ping(ctx: Context<'_>,) -> CommandResult {
    let ping_before = std::time::SystemTime::now();
    let ping_msg = ctx.say("Loading!").await?;
    let content = format!("Current Latency: {}ms", ping_before.elapsed()?.as_millis());

    match ping_msg {
        poise::ReplyHandle::Known(mut msg) => {
            msg.edit(ctx.discord(), |b| b.content(content)).await?;
        },
        poise::ReplyHandle::Unknown { http, interaction } => {
            interaction.edit_original_interaction_response(http, |b| {b.content(content)}).await?;  
        },
    }

    Ok(())
}

/// Suggests a new feature!
#[poise::command(category="Extra Commands", prefix_command, slash_command, required_bot_permissions="SEND_MESSAGES")]
pub async fn suggest(ctx: Context<'_>, #[description="the suggestion to submit"] #[rest] suggestion: String) -> CommandResult {
    if suggestion.to_lowercase().replace('<', ">") == "suggestion" {
        ctx.say("Hey! You are meant to replace `<suggestion>` with your actual suggestion!").await?;
        return Ok(())
    }

    let data = ctx.data();
    let author = ctx.author();
    if !data.userinfo_db.get(author.id.into()).await?.get::<_, bool>("dm_blocked") {
        data.webhooks["suggestions"].execute(&ctx.discord().http, false, |b| {b
            .content(suggestion)
            .avatar_url(author.face())
            .username(format!("{}#{} ({})", author.name, author.discriminator, author.id))
        }).await?;
    }

    ctx.say("Suggestion noted").await?;
    Ok(())
}

/// Sends the instructions to invite TTS Bot and join the support server!
#[poise::command(category="Extra Commands", prefix_command, slash_command, required_bot_permissions="SEND_MESSAGES")]
pub async fn invite(ctx: Context<'_>,) -> CommandResult {
    let ctx_discord = ctx.discord();
    let bot_user_id = ctx_discord.cache.current_user_id();

    let config = &ctx.data().config;
    let invite_channel = config.invite_channel;

    if ctx.guild_id() == Some(config.main_server) {
        ctx.say(format!("Check out <#{}> to invite <@{}>!", invite_channel, bot_user_id)).await?;
        return Ok(())
    }

    let invite_channel = ctx_discord.cache.guild_channel(invite_channel).try_unwrap()?;
    ctx.say(format!("Join {} and look in #{} to invite <@{}>", config.main_server_invite, invite_channel.name, bot_user_id)).await?;

    Ok(())
}
