use poise::serenity_prelude as serenity;

use tts_core::{
    constants::VIEW_TRACEBACK_CUSTOM_ID,
    structs::{FrameworkContext, Result},
};

#[derive(sqlx::FromRow)]
struct TracebackRow {
    pub traceback: String,
}

async fn handle_traceback_button(
    framework: FrameworkContext<'_>,
    interaction: &serenity::ComponentInteraction,
) -> Result<()> {
    let row: Option<TracebackRow> =
        sqlx::query_as("SELECT traceback FROM errors WHERE message_id = $1")
            .bind(interaction.message.id.get() as i64)
            .fetch_optional(&framework.user_data().pool)
            .await?;

    let mut response_data = serenity::CreateInteractionResponseMessage::default().ephemeral(true);
    response_data = if let Some(TracebackRow { traceback }) = row {
        response_data.files([serenity::CreateAttachment::bytes(
            traceback.into_bytes(),
            "traceback.txt",
        )])
    } else {
        response_data.content("No traceback found.")
    };

    interaction
        .create_response(
            &framework.serenity_context.http,
            serenity::CreateInteractionResponse::Message(response_data),
        )
        .await?;
    Ok(())
}

pub async fn interaction_create(
    framework_ctx: FrameworkContext<'_>,
    interaction: &serenity::Interaction,
) -> Result<()> {
    if let serenity::Interaction::Component(interaction) = interaction {
        if interaction.data.custom_id == VIEW_TRACEBACK_CUSTOM_ID {
            handle_traceback_button(framework_ctx, interaction).await?;
        };
    };

    Ok(())
}
