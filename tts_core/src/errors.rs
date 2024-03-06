use std::{borrow::Cow, sync::Arc};

use anyhow::{Error, Result};
use sha2::Digest;

use self::serenity::{
    small_fixed_array::{FixedString, TruncatingInto},
    CreateActionRow, CreateButton,
};
use poise::serenity_prelude as serenity;

use crate::{
    constants::{self, VIEW_TRACEBACK_CUSTOM_ID},
    opt_ext::OptionTryUnwrap,
    structs::{Data, FrameworkContext},
};

#[derive(sqlx::FromRow)]
struct ErrorRow {
    pub message_id: i64,
}

#[must_use]
pub const fn blank_field() -> (&'static str, Cow<'static, str>, bool) {
    ("\u{200B}", Cow::Borrowed("\u{200B}"), true)
}

fn hash(data: &[u8]) -> Vec<u8> {
    let mut hasher = sha2::Sha256::new();
    hasher.update(data);
    Vec::from(&*hasher.finalize())
}

fn truncate_error(error: &Error) -> String {
    let mut long_err = error.to_string();

    // Avoid char boundary panics with utf8 chars
    let mut new_len = 256;
    while !long_err.is_char_boundary(new_len) {
        new_len -= 1;
    }

    long_err.truncate(new_len);
    long_err
}

async fn fetch_update_occurrences(
    http: &serenity::Http,
    data: &Data,
    error: &Error,
) -> Result<Option<(String, Vec<u8>)>, Error> {
    #[derive(sqlx::FromRow)]
    struct ErrorRowWithOccurrences {
        pub message_id: i64,
        pub occurrences: i32,
    }

    let traceback = format!("{error:?}");
    let traceback_hash = hash(traceback.as_bytes());

    let query = "
        UPDATE errors SET occurrences = occurrences + 1
        WHERE traceback_hash = $1
        RETURNING message_id, occurrences";

    let Some(ErrorRowWithOccurrences {
        message_id,
        occurrences,
    }) = sqlx::query_as(query)
        .bind(traceback_hash.clone())
        .fetch_optional(&data.pool)
        .await?
    else {
        return Ok(Some((traceback, traceback_hash)));
    };

    let error_webhook = &data.webhooks.errors;
    let message_id = serenity::MessageId::new(message_id as u64);

    let message = error_webhook.get_message(http, None, message_id).await?;
    let mut embed = message.embeds.into_vec().remove(0);

    embed.footer.as_mut().try_unwrap()?.text =
        format!("This error has occurred {occurrences} times!").trunc_into();

    let builder = serenity::EditWebhookMessage::default().embeds(vec![embed.into()]);
    error_webhook
        .edit_message(http, message_id, builder)
        .await?;

    Ok(None)
}

async fn insert_traceback(
    http: &serenity::Http,
    data: &Data,
    embed: serenity::CreateEmbed<'_>,
    traceback: String,
    traceback_hash: Vec<u8>,
) -> Result<()> {
    let button = CreateButton::new(VIEW_TRACEBACK_CUSTOM_ID)
        .label("View Traceback")
        .style(serenity::ButtonStyle::Danger);

    let embeds = [embed];
    let components = [CreateActionRow::Buttons(vec![button])];

    let builder = serenity::ExecuteWebhook::default()
        .embeds(embeds.as_slice())
        .components(components.as_slice());

    let message = data
        .webhooks
        .errors
        .execute(http, true, builder)
        .await?
        .try_unwrap()?;

    let ErrorRow {
        message_id: db_message_id,
    } = sqlx::query_as(
        "INSERT INTO errors(traceback_hash, traceback, message_id)
        VALUES($1, $2, $3)

        ON CONFLICT (traceback_hash)
        DO UPDATE SET occurrences = errors.occurrences + 1
        RETURNING errors.message_id",
    )
    .bind(traceback_hash)
    .bind(traceback)
    .bind(message.id.get() as i64)
    .fetch_one(&data.pool)
    .await?;

    if message.id != db_message_id as u64 {
        data.webhooks
            .errors
            .delete_message(http, None, message.id)
            .await?;
    }

    Ok(())
}

pub async fn handle_unexpected<'a>(
    poise_context: FrameworkContext<'_>,
    event: &'a str,
    error: Error,
    // Split out logic if not reliant on this field, to prevent monomorphisation bloat
    extra_fields: impl IntoIterator<Item = (&str, Cow<'a, str>, bool)>,
    author_name: Option<&str>,
    icon_url: Option<&str>,
) -> Result<()> {
    let data = poise_context.user_data();
    let ctx = poise_context.serenity_context;

    let Some((traceback, traceback_hash)) =
        fetch_update_occurrences(&ctx.http, &data, &error).await?
    else {
        return Ok(());
    };

    let (cpu_usage, mem_usage) = {
        let mut system = data.system_info.lock();
        system.refresh_specifics(
            sysinfo::RefreshKind::new().with_memory(sysinfo::MemoryRefreshKind::new().with_ram()),
        );

        (
            sysinfo::System::load_average().five.to_string(),
            (system.used_memory() / 1024).to_string(),
        )
    };

    let before_fields = [
        ("Event", Cow::Borrowed(event), true),
        (
            "Bot User",
            Cow::Owned(ctx.cache.current_user().name.to_string()),
            true,
        ),
        blank_field(),
    ];

    let shards = poise_context.shard_manager.shards_instantiated().await;
    let after_fields = [
        ("CPU Usage (5 minutes)", Cow::Owned(cpu_usage), true),
        ("System Memory Usage", Cow::Owned(mem_usage), true),
        ("Shard Count", Cow::Owned(shards.len().to_string()), true),
    ];

    let footer = serenity::CreateEmbedFooter::new("This error has occurred 1 time!");
    let mut embed = serenity::CreateEmbed::default()
        .colour(constants::RED)
        .title(truncate_error(&error))
        .footer(footer);

    for (title, mut value, inline) in before_fields
        .into_iter()
        .chain(extra_fields)
        .chain(after_fields)
    {
        if value != "\u{200B}" {
            let value = value.to_mut();
            value.insert(0, '`');
            value.push('`');
        };

        embed = embed.field(title, value, inline);
    }

    if let Some(author_name) = author_name {
        let mut author_builder = serenity::CreateEmbedAuthor::new(author_name);
        if let Some(icon_url) = icon_url {
            author_builder = author_builder.icon_url(icon_url);
        }

        embed = embed.author(author_builder);
    }

    insert_traceback(&ctx.http, &data, embed, traceback, traceback_hash).await
}

struct TrackErrorHandler<Iter: IntoIterator<Item = (&'static str, Cow<'static, str>, bool)>> {
    ctx: serenity::Context,
    shard_manager: Arc<serenity::ShardManager>,
    extra_fields: Iter,
    author_name: FixedString<u8>,
    icon_url: String,
}

#[serenity::async_trait]
impl<Iter> songbird::EventHandler for TrackErrorHandler<Iter>
where
    Iter: IntoIterator<Item = (&'static str, Cow<'static, str>, bool)> + Clone + Send + Sync,
{
    async fn act(&self, ctx: &songbird::EventContext<'_>) -> Option<songbird::Event> {
        if let songbird::EventContext::Track([(state, _)]) = ctx {
            if let songbird::tracks::PlayMode::Errored(error) = state.playing.clone() {
                // HACK: Cannot get reference to options from here, so has to be faked.
                // This is fine because the options are not used in the error handler.
                let framework_context = FrameworkContext {
                    serenity_context: &self.ctx,
                    shard_manager: &self.shard_manager,
                    options: &poise::FrameworkOptions::default(),
                };

                let author_name = Some(self.author_name.as_str());
                let icon_url = Some(self.icon_url.as_str());

                let result = handle_unexpected(
                    framework_context,
                    "TrackError",
                    error.into(),
                    self.extra_fields.clone(),
                    author_name,
                    icon_url,
                )
                .await;

                if let Err(err_err) = result {
                    tracing::error!("Songbird unhandled track error: {err_err}");
                }
            }
        }

        Some(songbird::Event::Cancel)
    }
}

/// Registers a track to be handled by the error handler, arguments other than the
/// track are passed to [`handle_unexpected`] if the track errors.
pub fn handle_track<Iter>(
    ctx: serenity::Context,
    shard_manager: Arc<serenity::ShardManager>,
    extra_fields: Iter,
    author_name: FixedString<u8>,
    icon_url: String,

    track: &songbird::tracks::TrackHandle,
) -> Result<(), songbird::error::ControlError>
where
    Iter: IntoIterator<Item = (&'static str, Cow<'static, str>, bool)>
        + Clone
        + Send
        + Sync
        + 'static,
{
    track.add_event(
        songbird::Event::Track(songbird::TrackEvent::Error),
        TrackErrorHandler {
            ctx,
            shard_manager,
            extra_fields,
            author_name,
            icon_url,
        },
    )
}
