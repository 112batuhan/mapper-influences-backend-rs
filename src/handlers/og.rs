use std::sync::{Arc, LazyLock};

use axum::{
    extract::{Path, State},
    response::Html,
};

use crate::{database::user::User, error::AppError, AppState};

use super::PathUserId;

static FRONTEND_URL: LazyLock<String> = LazyLock::new(|| {
    std::env::var("FRONTEND_URL").unwrap_or_else(|_| "https://www.mapperinfluences.com".to_string())
});

const MAX_DESCRIPTION_LENGTH: usize = 160;

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

fn count_label(count: u32, singular: &str) -> String {
    if count == 1 {
        format!("{count} {singular}")
    } else {
        format!("{count} {singular}s")
    }
}

/// Stat row for the embed. Empty fields and zero counts are left out, so a
/// sparse profile does not render stray separators.
fn user_metadata(user: &User) -> String {
    let mut parts: Vec<String> = Vec::new();

    let country = user.country_name.trim();
    if !country.is_empty() {
        parts.push(country.to_string());
    }

    let ranked_maps = user.ranked_and_approved_beatmapset_count + user.guest_beatmapset_count;
    if ranked_maps > 0 {
        parts.push(count_label(ranked_maps, "ranked map"));
    }

    if let Some(influences) = user.influences.filter(|count| *count > 0) {
        parts.push(count_label(influences, "influence"));
    }

    if let Some(mentions) = user.mentions.filter(|count| *count > 0) {
        parts.push(count_label(mentions, "mention"));
    }

    parts.join(" · ")
}

/// Description body of the embed: the stat row, then the bio on its own line.
/// Link preview clients render the newline as a line break.
fn user_description(user: &User) -> String {
    let metadata = user_metadata(user);
    let bio = summarize(&user.bio, MAX_DESCRIPTION_LENGTH);

    match (metadata.is_empty(), bio.is_empty()) {
        (true, true) => "Track and share your osu! mapping influences.".to_string(),
        (true, false) => bio,
        (false, true) => metadata,
        (false, false) => format!("{metadata}\n{bio}"),
    }
}

/// Open Graph meta page for link embeds (Discord, Twitter and similar).
/// Meant to be served to link preview crawlers only, humans get redirected
/// to the frontend profile page by the meta refresh tag.
pub async fn get_user_og(
    Path(user_id): Path<PathUserId>,
    State(state): State<Arc<AppState>>,
) -> Result<Html<String>, AppError> {
    let page_url = format!("{}/profile/{}", *FRONTEND_URL, user_id.value);

    match state.db.get_user_details(user_id.value).await {
        Ok(user) => Ok(Html(render_meta(
            &format!("{} | Mapper Influences", user.username),
            &user_description(&user),
            &user.avatar_url,
            &page_url,
        ))),
        // Fall back to the generic site embed for users that are not in the database
        Err(AppError::MissingUser(_)) => Ok(Html(render_meta(
            "Mapper Influences",
            "Track and share your osu! mapping influences.",
            &format!("{}/icon-512.png", *FRONTEND_URL),
            &page_url,
        ))),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user() -> User {
        User {
            id: 1,
            username: "Mapper".to_string(),
            avatar_url: "https://a.ppy.sh/1".to_string(),
            bio: String::new(),
            groups: Vec::new(),
            country_code: "TR".to_string(),
            country_name: "Turkey".to_string(),
            previous_usernames: Vec::new(),
            ranked_and_approved_beatmapset_count: 0,
            ranked_beatmapset_count: 0,
            nominated_beatmapset_count: 0,
            guest_beatmapset_count: 0,
            loved_beatmapset_count: 0,
            graveyard_beatmapset_count: 0,
            pending_beatmapset_count: 0,
            beatmaps: Vec::new(),
            mentions: None,
            influences: None,
        }
    }

    #[test]
    fn metadata_skips_empty_and_zero_fields() {
        let mut sparse = user();
        sparse.country_name = String::new();
        assert_eq!(user_metadata(&sparse), "");

        let mut filled = user();
        filled.ranked_and_approved_beatmapset_count = 3;
        filled.guest_beatmapset_count = 1;
        filled.influences = Some(1);
        filled.mentions = Some(0);
        assert_eq!(
            user_metadata(&filled),
            "Turkey · 4 ranked maps · 1 influence"
        );
    }

    #[test]
    fn description_puts_bio_on_its_own_line() {
        let mut user = user();
        user.bio = "  Loves\n  jumps  ".to_string();
        user.influences = Some(2);
        assert_eq!(
            user_description(&user),
            "Turkey · 2 influences\nLoves jumps"
        );
    }

    #[test]
    fn description_falls_back_when_everything_is_empty() {
        let mut empty = user();
        empty.country_name = String::new();
        assert_eq!(
            user_description(&empty),
            "Track and share your osu! mapping influences."
        );
    }
}
