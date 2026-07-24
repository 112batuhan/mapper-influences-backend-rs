use std::sync::Arc;

use axum::{
    extract::{Path, State},
    response::Html,
};

use crate::{database::user::User, error::AppError, AppState};

use super::PathUserId;

const DEFAULT_FRONTEND_URL: &str = "https://www.mapperinfluences.com";
const MAX_DESCRIPTION_LENGTH: usize = 160;

fn frontend_url() -> String {
    std::env::var("FRONTEND_URL").unwrap_or_else(|_| DEFAULT_FRONTEND_URL.to_string())
}

fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Collapses whitespace runs and truncates to a character limit for meta descriptions
fn summarize(text: &str, max_chars: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let truncated: String = collapsed.chars().take(max_chars - 1).collect();
    format!("{}…", truncated.trim_end())
}

fn render_meta(title: &str, description: &str, image: &str, page_url: &str) -> String {
    let title = escape_html(title);
    let description = escape_html(description);
    let image = escape_html(image);
    let page_url = escape_html(page_url);
    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n\
        <meta charset=\"utf-8\">\n\
        <title>{title}</title>\n\
        <meta name=\"description\" content=\"{description}\">\n\
        <meta property=\"og:title\" content=\"{title}\">\n\
        <meta property=\"og:description\" content=\"{description}\">\n\
        <meta property=\"og:type\" content=\"profile\">\n\
        <meta property=\"og:url\" content=\"{page_url}\">\n\
        <meta property=\"og:image\" content=\"{image}\">\n\
        <meta name=\"twitter:card\" content=\"summary\">\n\
        <meta http-equiv=\"refresh\" content=\"0; url={page_url}\">\n\
        </head>\n<body></body>\n</html>"
    )
}

fn user_description(user: &User) -> String {
    if !user.bio.trim().is_empty() {
        return summarize(&user.bio, MAX_DESCRIPTION_LENGTH);
    }
    match user.mentions {
        Some(mentions) if mentions > 0 => format!(
            "Mentioned as an influence by {} mapper{} on Mapper Influences.",
            mentions,
            if mentions == 1 { "" } else { "s" }
        ),
        _ => "Track and share your osu! mapping influences.".to_string(),
    }
}

/// Open Graph meta page for link embeds (Discord, Twitter and similar).
/// Meant to be served to link preview crawlers only, humans get redirected
/// to the frontend profile page by the meta refresh tag.
pub async fn get_user_og(
    Path(user_id): Path<PathUserId>,
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, AppError> {
    let frontend = frontend_url();
    let page_url = format!("{}/profile/{}", frontend, user_id.value);

    match state.db.get_user_details(user_id.value).await {
        Ok(user) => Ok(Html(render_meta(
            &format!("{} — Mapper Influences", user.username),
            &user_description(&user),
            &user.avatar_url,
            &page_url,
        ))),
        // Fall back to the generic site embed for users that are not in the database
        Err(AppError::MissingUser(_)) => Ok(Html(render_meta(
            "Mapper Influences",
            "Track and share your osu! mapping influences.",
            &format!("{}/icon-512.png", frontend),
            &page_url,
        ))),
        Err(error) => Err(error),
    }
}
