use std::{future::Future, sync::Arc, time::Duration};

use tokio::time::{Instant, MissedTickBehavior};
use tracing::{debug, error};

use crate::{
    error::AppError,
    handlers::leaderboard::{
        beatmap_leaderboard_key, build_beatmap_leaderboard, build_user_leaderboard,
        user_leaderboard_key,
    },
    AppState,
};

/// The leaderboards move with every new influence, so they are kept close to fresh.
pub const LEADERBOARD_REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// The graph is a much heavier query over every user and influence, so it's given
/// a longer interval than the leaderboards.
pub const GRAPH_REFRESH_INTERVAL: Duration = Duration::from_secs(20 * 60);

/// How far apart the updaters are spaced. They all hit the database hard, so past
/// the initial warmup they are kept out of each other's way.
pub const UPDATER_START_STAGGER: Duration = Duration::from_secs(20);

/// Country specific leaderboards are left to the handlers, there is no reasonable
/// way to guess which countries are worth keeping warm.
pub async fn refresh_user_leaderboard(state: &AppState, ranked: bool) -> Result<(), AppError> {
    let leaderboard = build_user_leaderboard(state, ranked, None).await?;
    state
        .user_leaderboard_cache
        .add_leaderboard(&user_leaderboard_key(ranked, None), &leaderboard)
        .await
}

pub async fn refresh_beatmap_leaderboard(state: &AppState, ranked: bool) -> Result<(), AppError> {
    let leaderboard = build_beatmap_leaderboard(state, ranked).await?;
    state
        .beatmap_leaderboard_cache
        .add_leaderboard(&beatmap_leaderboard_key(ranked), &leaderboard)
        .await
}

pub async fn refresh_graph(state: &AppState) -> Result<(), AppError> {
    let graph_data = state.db.get_graph_data().await?;
    state.graph_cache.update(&graph_data).await
}

/// The base updater every cache gets its own copy of. It rebuilds the entry once
/// right away and then on its own interval, offset by `start_delay`.
///
/// The first rebuild ignores the delay, there is no traffic to protect yet and a
/// cold cache is worth more than the spacing. From the second one on, the schedule
/// is `start_delay` into every interval, which is what keeps the updaters apart.
///
/// A failing cycle is logged and waited out, the next tick tries again. The entry
/// outlives its interval in redis, so the endpoint keeps serving the previous data
/// in the meantime.
pub async fn update_routine<F, Fut>(
    name: &'static str,
    interval: Duration,
    start_delay: Duration,
    state: Arc<AppState>,
    refresh: F,
) where
    F: Fn(Arc<AppState>) -> Fut,
    Fut: Future<Output = Result<(), AppError>>,
{
    let started = Instant::now();

    run_refresh(name, &state, &refresh).await;

    // Anchored to the startup instant rather than to now, so however long the first
    // rebuild took doesn't shift this updater into another one's slot
    let mut interval = tokio::time::interval_at(started + start_delay + interval, interval);
    // A rebuild that overruns its interval must not be followed by a catch up burst,
    // that would undo the spacing and pile the queries back on top of each other
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        interval.tick().await;
        run_refresh(name, &state, &refresh).await;
    }
}

async fn run_refresh<F, Fut>(name: &'static str, state: &Arc<AppState>, refresh: &F)
where
    F: Fn(Arc<AppState>) -> Fut,
    Fut: Future<Output = Result<(), AppError>>,
{
    match refresh(state.clone()).await {
        Ok(()) => debug!("refreshed the {} cache", name),
        Err(error) => error!("failed to refresh the {} cache: {}", name, error),
    }
}

/// One updater per cached entry, so a slow or failing one doesn't hold up the rest.
///
/// All of them rebuild once at startup to warm the cache, and are `stagger` apart
/// from their second cycle on, because these rebuilds are heavy on the database. The
/// leaderboard updaters share an interval, so that spacing is the spacing they keep.
/// The graph interval is a multiple of the leaderboard one, which keeps it out of
/// their slots as well.
pub fn spawn_cache_updaters(state: Arc<AppState>, stagger: Duration) {
    tokio::spawn(update_routine(
        "user leaderboard",
        LEADERBOARD_REFRESH_INTERVAL,
        Duration::ZERO,
        state.clone(),
        |state| async move { refresh_user_leaderboard(&state, false).await },
    ));
    tokio::spawn(update_routine(
        "ranked user leaderboard",
        LEADERBOARD_REFRESH_INTERVAL,
        stagger,
        state.clone(),
        |state| async move { refresh_user_leaderboard(&state, true).await },
    ));
    tokio::spawn(update_routine(
        "beatmap leaderboard",
        LEADERBOARD_REFRESH_INTERVAL,
        2 * stagger,
        state.clone(),
        |state| async move { refresh_beatmap_leaderboard(&state, false).await },
    ));
    tokio::spawn(update_routine(
        "ranked beatmap leaderboard",
        LEADERBOARD_REFRESH_INTERVAL,
        3 * stagger,
        state.clone(),
        |state| async move { refresh_beatmap_leaderboard(&state, true).await },
    ));
    tokio::spawn(update_routine(
        "graph data",
        GRAPH_REFRESH_INTERVAL,
        4 * stagger,
        state,
        |state| async move { refresh_graph(&state).await },
    ));
}
