use std::borrow::Cow;

use poise::serenity_prelude as serenity;

use crate::{
    errors,
    structs::{Data, Result, SerenityContext},
};

pub fn resume(data: &Data) {
    data.analytics.log(Cow::Borrowed("resumed"), false);
}

pub async fn interaction_create(
    ctx: &SerenityContext,
    interaction: &serenity::Interaction,
) -> Result<()> {
    errors::interaction_create(ctx, interaction).await
}
