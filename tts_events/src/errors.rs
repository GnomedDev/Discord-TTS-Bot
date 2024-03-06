use std::borrow::Cow;

use anyhow::Error;
use serenity::all::{self as serenity, FullEvent as Event};

use tracing::error;
use tts_core::{
    errors::{blank_field, handle_unexpected},
    require,
    structs::{Context, FrameworkContext, FrameworkError, Result},
    traits::PoiseContextExt as _,
    translations::GetTextContextExt as _,
};

async fn handle_unexpected_default(
    framework: FrameworkContext<'_>,
    name: &str,
    result: Result<()>,
) -> Result<()> {
    let error = require!(result.err(), Ok(()));

    handle_unexpected(framework, name, error, [], None, None).await
}

// Listener Handlers
async fn handle_message(
    poise_context: FrameworkContext<'_>,
    message: &serenity::Message,
    result: Result<impl Send + Sync>,
) -> Result<()> {
    let error = require!(result.err(), Ok(()));
    let ctx = poise_context.serenity_context;

    let mut extra_fields = Vec::with_capacity(3);
    if let Some(guild_id) = message.guild_id {
        if let Some(guild_name) = ctx.cache.guild(guild_id).map(|g| g.name.to_string()) {
            extra_fields.push(("Guild", Cow::Owned(guild_name), true));
        }

        extra_fields.push(("Guild ID", Cow::Owned(guild_id.to_string()), true));
    }

    extra_fields.push((
        "Channel Type",
        Cow::Borrowed(channel_type(&message.channel_id.to_channel(&ctx).await?)),
        true,
    ));
    handle_unexpected(
        poise_context,
        "MessageCreate",
        error,
        extra_fields,
        Some(&message.author.name),
        Some(&message.author.face()),
    )
    .await
}

async fn handle_member(
    framework: FrameworkContext<'_>,
    member: &serenity::Member,
    result: Result<(), impl Into<Error>>,
) -> Result<()> {
    let error = require!(result.err(), Ok(())).into();

    let extra_fields = [
        ("Guild", Cow::Owned(member.guild_id.to_string()), true),
        ("Guild ID", Cow::Owned(member.guild_id.to_string()), true),
        ("User ID", Cow::Owned(member.user.id.to_string()), true),
    ];

    handle_unexpected(framework, "GuildMemberAdd", error, extra_fields, None, None).await
}

async fn handle_guild(
    name: &str,
    framework: FrameworkContext<'_>,
    guild: Option<&serenity::Guild>,
    result: Result<()>,
) -> Result<()> {
    let error = require!(result.err(), Ok(()));

    handle_unexpected(
        framework,
        name,
        error,
        [],
        guild.as_ref().map(|g| g.name.as_str()),
        guild.and_then(serenity::Guild::icon_url).as_deref(),
    )
    .await
}

// Command Error handlers
async fn handle_cooldown(ctx: Context<'_>, remaining_cooldown: std::time::Duration) -> Result<()> {
    let cooldown_response = ctx
        .send_error(
            ctx.gettext("`/{command_name}` is on cooldown, please try again in {} seconds!")
                .replace("{command_name}", &ctx.command().name)
                .replace("{}", &format!("{:.1}", remaining_cooldown.as_secs_f32())),
        )
        .await?;

    if let poise::Context::Prefix(ctx) = ctx {
        if let Some(error_reply) = cooldown_response {
            // Never actually fetches, as Prefix already has message.
            let error_message = error_reply.into_message().await?;
            tokio::time::sleep(remaining_cooldown).await;

            let ctx_discord = ctx.serenity_context();
            error_message.delete(ctx_discord).await?;

            let bot_user_id = ctx_discord.cache.current_user().id;
            let Some(channel) = error_message.channel(ctx_discord).await?.guild() else {
                return Ok(());
            };

            if channel
                .permissions_for_user(&ctx_discord.cache, bot_user_id)?
                .manage_messages()
            {
                ctx.msg.delete(ctx_discord).await?;
            }
        }
    };

    Ok(())
}

async fn handle_argparse(
    ctx: Context<'_>,
    error: Box<dyn std::error::Error + Send + Sync>,
    input: Option<String>,
) -> Result<(), Error> {
    let reason = if let Some(input) = input {
        let reason = if error.is::<serenity::MemberParseError>() {
            ctx.gettext("I cannot find the member: `{}`")
        } else if error.is::<serenity::GuildParseError>() {
            ctx.gettext("I cannot find the server: `{}`")
        } else if error.is::<serenity::GuildChannelParseError>() {
            ctx.gettext("I cannot find the channel: `{}`")
        } else if error.is::<std::num::ParseIntError>() {
            ctx.gettext("I cannot convert `{}` to a number")
        } else if error.is::<std::str::ParseBoolError>() {
            ctx.gettext("I cannot convert `{}` to True/False")
        } else {
            ctx.gettext("I cannot understand your message")
        };

        Cow::Owned(reason.replace("{}", &input))
    } else {
        Cow::Borrowed(ctx.gettext("You missed an argument to the command"))
    };

    let fix = ctx
        .gettext("please check out `/help {command}`")
        .replace("{command}", &ctx.command().qualified_name);

    ctx.send_error(format!("{reason}, {fix}")).await?;
    Ok(())
}

const fn channel_type(channel: &serenity::Channel) -> &'static str {
    use self::serenity::{Channel, ChannelType};

    match channel {
        Channel::Guild(channel) => match channel.kind {
            ChannelType::Text | ChannelType::News => "Text Channel",
            ChannelType::Voice => "Voice Channel",
            ChannelType::NewsThread => "News Thread Channel",
            ChannelType::PublicThread => "Public Thread Channel",
            ChannelType::PrivateThread => "Private Thread Channel",
            _ => "Unknown Channel Type",
        },
        Channel::Private(_) => "Private Channel",
        _ => "Unknown Channel Type",
    }
}

pub async fn handle(error: FrameworkError<'_>) -> Result<()> {
    match error {
        poise::FrameworkError::DynamicPrefix { error, .. } => {
            error!("Error in dynamic_prefix: {:?}", error);
        }
        poise::FrameworkError::Command { error, ctx, .. } => {
            let author = ctx.author();
            let command = ctx.command();

            let mut extra_fields = vec![
                ("Command", Cow::Owned(command.name.clone()), true),
                (
                    "Slash Command",
                    Cow::Owned(matches!(ctx, poise::Context::Application(..)).to_string()),
                    true,
                ),
                (
                    "Channel Type",
                    Cow::Borrowed(channel_type(&ctx.channel_id().to_channel(&ctx).await?)),
                    true,
                ),
            ];

            if let Some(guild) = ctx.guild() {
                extra_fields.extend([
                    ("Guild", Cow::Owned(guild.name.to_string()), true),
                    ("Guild ID", Cow::Owned(guild.id.to_string()), true),
                    blank_field(),
                ]);
            }

            handle_unexpected(
                ctx.framework(),
                "command",
                error,
                extra_fields,
                Some(&author.name),
                Some(&author.face()),
            )
            .await?;

            let msg =
                ctx.gettext("An unknown error occurred, please report this on the support server!");
            ctx.send_error(msg).await?;
        }
        poise::FrameworkError::ArgumentParse {
            error, ctx, input, ..
        } => handle_argparse(ctx, error, input).await?,
        poise::FrameworkError::CooldownHit {
            remaining_cooldown,
            ctx,
            ..
        } => handle_cooldown(ctx, remaining_cooldown).await?,
        poise::FrameworkError::MissingBotPermissions {
            missing_permissions,
            ctx,
            ..
        } => {
            let msg = ctx.gettext("I cannot run this command as I am missing permissions, please ask an administrator of the server to give me: {}")
                .replace("{}", &missing_permissions.get_permission_names().join(", "));

            ctx.send_error(msg).await?;
        }
        poise::FrameworkError::MissingUserPermissions {
            missing_permissions,
            ctx,
            ..
        } => {
            let msg = if let Some(missing_permissions) = missing_permissions {
                Cow::Owned(ctx.gettext("You cannot run this command as you are missing permissions, please ask an administrator of the server to give you: {}")
                    .replace("{}", &missing_permissions.get_permission_names().join(", ")))
            } else {
                Cow::Borrowed(
                    ctx.gettext("You cannot run this command as you are missing permissions."),
                )
            };

            ctx.send_error(msg).await?;
        }

        poise::FrameworkError::CommandCheckFailed { error, ctx, .. } => {
            if let Some(error) = error {
                error!("Premium Check Error: {:?}", error);

                let msg = ctx.gettext("An unknown error occurred during the premium check, please report this on the support server!");
                ctx.send_error(msg).await?;
            }
        }

        poise::FrameworkError::EventHandler {
            error,
            event,
            framework,
            ..
        } => {
            #[allow(non_snake_case)]
            fn Err<E>(error: E) -> Result<(), E> {
                Result::Err(error)
            }

            match event {
                Event::Message { new_message } => {
                    handle_message(framework, new_message, Err(error)).await?;
                }
                Event::GuildMemberAddition { new_member } => {
                    handle_member(framework, new_member, Err(error)).await?;
                }
                Event::GuildCreate { guild, .. } => {
                    handle_guild("GuildCreate", framework, Some(guild), Err(error)).await?;
                }
                Event::GuildDelete { full, .. } => {
                    handle_guild("GuildDelete", framework, full.as_ref(), Err(error)).await?;
                }
                Event::VoiceStateUpdate { .. } => {
                    handle_unexpected_default(framework, "VoiceStateUpdate", Err(error)).await?;
                }
                Event::InteractionCreate { .. } => {
                    handle_unexpected_default(framework, "InteractionCreate", Err(error)).await?;
                }
                Event::Ready { .. } => {
                    handle_unexpected_default(framework, "Ready", Err(error)).await?;
                }
                _ => {
                    tracing::warn!("Unhandled {} error: {:?}", event.snake_case_name(), error);
                }
            }
        }
        poise::FrameworkError::CommandStructureMismatch { .. }
        | poise::FrameworkError::DmOnly { .. }
        | poise::FrameworkError::NsfwOnly { .. }
        | poise::FrameworkError::NotAnOwner { .. }
        | poise::FrameworkError::UnknownInteraction { .. }
        | poise::FrameworkError::SubcommandRequired { .. }
        | poise::FrameworkError::UnknownCommand { .. }
        | poise::FrameworkError::NonCommandMessage { .. } => {}
        poise::FrameworkError::GuildOnly { ctx, .. } => {
            let error = ctx
                .gettext("`/{command_name}` cannot be used in private messages, please run this command in a server channel.")
                .replace("{bot_name}", &ctx.cache().current_user().name)
                .replace("{command_name}", &ctx.command().qualified_name);

            ctx.send_error(error).await?;
        }
        poise::FrameworkError::CommandPanic { .. } => panic!("Command panicked!"),
        poise::FrameworkError::__NonExhaustive(_) => unreachable!(),
    }

    Ok(())
}
