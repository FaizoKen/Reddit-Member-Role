use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::Mutex;

use crate::error::RedditError;
use crate::services::reddit_oauth::RedditOAuth;
use crate::services::sync::PlayerSyncEvent;
use crate::AppState;

/// Reddit allows 60 req/min. We use 1 req/sec (conservative).
/// This controls how many *users* we refresh per hour, accounting for
/// multiple API calls per user (profile + karma breakdown + subreddit checks).
fn max_users_per_hour() -> i64 {
    std::env::var("REDDIT_MAX_USERS_PER_HOUR")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200)
}

const MIN_REFRESH_SECS: i64 = 1800; // 30 min floor
const MAX_REFRESH_SECS: i64 = 86400; // 24 hour cap
const INTERVAL_CACHE_SECS: u64 = 300; // recompute every 5 minutes

/// Inactive users (no role_assignments) are refreshed this many times slower.
const INACTIVE_MULTIPLIER: i64 = 6;

/// Caches the refresh interval to avoid running COUNT(*) on every fetch cycle.
struct CachedInterval {
    value: AtomicI64,
    max_users_per_hour: i64,
    last_computed: Mutex<Instant>,
}

impl CachedInterval {
    fn new(max_users_per_hour: i64) -> Self {
        Self {
            value: AtomicI64::new(MIN_REFRESH_SECS),
            max_users_per_hour,
            last_computed: Mutex::new(
                Instant::now() - std::time::Duration::from_secs(INTERVAL_CACHE_SECS + 1),
            ),
        }
    }

    async fn get(&self, pool: &sqlx::PgPool) -> i64 {
        let mut last = self.last_computed.lock().await;
        if last.elapsed() >= std::time::Duration::from_secs(INTERVAL_CACHE_SECS) {
            let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM linked_accounts")
                .fetch_one(pool)
                .await
                .unwrap_or(0);

            let interval = if user_count == 0 {
                MIN_REFRESH_SECS
            } else {
                ((user_count * 3600) / self.max_users_per_hour)
                    .clamp(MIN_REFRESH_SECS, MAX_REFRESH_SECS)
            };

            self.value.store(interval, Ordering::Relaxed);
            *last = Instant::now();
        }
        self.value.load(Ordering::Relaxed)
    }
}

pub async fn run(state: Arc<AppState>) {
    let max_users = max_users_per_hour();
    tracing::info!(max_users, "Refresh worker started");

    let cached_interval = CachedInterval::new(max_users);

    loop {
        // Wait for rate limiter before each cycle
        state.reddit_client.wait_for_permit().await;

        // Pick next user due for refresh, prioritizing active users
        let next = sqlx::query_as::<_, (String, String, bool)>(
            "SELECT uc.reddit_id, la.discord_id, \
             EXISTS(SELECT 1 FROM role_assignments ra WHERE ra.discord_id = la.discord_id) as is_active \
             FROM user_cache uc \
             JOIN linked_accounts la ON la.reddit_id = uc.reddit_id \
             WHERE uc.next_fetch_at <= now() \
             ORDER BY is_active DESC, uc.fetch_failures ASC, uc.next_fetch_at ASC \
             LIMIT 1",
        )
        .fetch_optional(&state.pool)
        .await;

        let (reddit_id, discord_id, is_active) = match next {
            Ok(Some(row)) => row,
            Ok(None) => {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                continue;
            }
            Err(e) => {
                tracing::error!("Refresh worker DB error: {e}");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        tracing::debug!(reddit_id, discord_id, is_active, "Refreshing Reddit data");

        // Get refresh token
        let refresh_token = match sqlx::query_scalar::<_, String>(
            "SELECT refresh_token FROM reddit_tokens WHERE reddit_id = $1",
        )
        .bind(&reddit_id)
        .fetch_optional(&state.pool)
        .await
        {
            Ok(Some(rt)) => rt,
            Ok(None) => {
                tracing::warn!(reddit_id, "No refresh token found, skipping");
                backoff_user(&state, &reddit_id).await;
                continue;
            }
            Err(e) => {
                tracing::error!(reddit_id, "DB error fetching refresh token: {e}");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        // Refresh the access token
        let reddit_oauth = RedditOAuth::with_client(state.http.clone());
        let access_token = match reddit_oauth
            .refresh_access_token(&state.config, &refresh_token)
            .await
        {
            Ok(token) => token,
            Err(e) => {
                tracing::warn!(reddit_id, "Token refresh failed (revoked?): {e}");
                // Don't delete the token — user might re-auth. Just backoff heavily.
                backoff_user(&state, &reddit_id).await;
                continue;
            }
        };

        // Fetch fresh user profile data
        match state.reddit_client.fetch_user_data(&access_token).await {
            Ok(user_data) => {
                let account_age_days = ((chrono::Utc::now().timestamp() as f64
                    - user_data.created_utc)
                    / 86400.0) as i32;

                let base_interval = cached_interval.get(&state.pool).await;
                let multiplier = if is_active { 1 } else { INACTIVE_MULTIPLIER };
                let ttl = base_interval * multiplier;
                let next_fetch = chrono::Utc::now() + chrono::Duration::seconds(ttl);

                if let Err(e) = sqlx::query(
                    "UPDATE user_cache SET \
                     user_data = $1, total_karma = $2, post_karma = $3, comment_karma = $4, \
                     account_age_days = $5, email_verified = $6, has_premium = $7, \
                     fetched_at = now(), next_fetch_at = $8, fetch_failures = 0 \
                     WHERE reddit_id = $9",
                )
                .bind(serde_json::json!({
                    "name": user_data.name,
                    "id": user_data.id,
                    "total_karma": user_data.total_karma,
                    "link_karma": user_data.link_karma,
                    "comment_karma": user_data.comment_karma,
                    "created_utc": user_data.created_utc,
                    "has_verified_email": user_data.has_verified_email,
                    "is_gold": user_data.is_gold,
                }))
                .bind(user_data.total_karma as i32)
                .bind(user_data.link_karma as i32)
                .bind(user_data.comment_karma as i32)
                .bind(account_age_days)
                .bind(user_data.has_verified_email)
                .bind(user_data.is_gold)
                .bind(next_fetch)
                .bind(&reddit_id)
                .execute(&state.pool)
                .await
                {
                    tracing::error!(reddit_id, "Failed to update user cache: {e}");
                    continue;
                }

                // Fetch karma breakdown
                if let Ok(karma_list) = state
                    .reddit_client
                    .fetch_karma_breakdown(&access_token)
                    .await
                {
                    for k in &karma_list {
                        let sub = k.sr.to_lowercase();
                        let _ = sqlx::query(
                            "INSERT INTO user_subreddit_data (reddit_id, subreddit, post_karma, comment_karma) \
                             VALUES ($1, $2, $3, $4) \
                             ON CONFLICT (reddit_id, subreddit) DO UPDATE SET \
                             post_karma = $3, comment_karma = $4, fetched_at = now()",
                        )
                        .bind(&reddit_id)
                        .bind(&sub)
                        .bind(k.link_karma as i32)
                        .bind(k.comment_karma as i32)
                        .execute(&state.pool)
                        .await;
                    }
                }

                // Refresh tracked subreddit data (subscriber/moderator status, post/comment counts)
                refresh_tracked_subreddits(
                    &state,
                    &reddit_id,
                    &user_data.name,
                    &access_token,
                )
                .await;

                // Trigger player sync
                let _ = state
                    .player_sync_tx
                    .send(PlayerSyncEvent::PlayerUpdated {
                        discord_id: discord_id.clone(),
                    })
                    .await;

                tracing::debug!(reddit_id, ttl, is_active, "Reddit data refreshed");
            }
            Err(RedditError::TokenRevoked) => {
                tracing::warn!(reddit_id, "Reddit token revoked, backing off");
                backoff_user(&state, &reddit_id).await;
            }
            Err(RedditError::RateLimited) => {
                tracing::warn!("Reddit rate limited, backing off 5s");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
            Err(RedditError::Suspended) => {
                tracing::warn!(reddit_id, "Reddit account suspended, backing off");
                backoff_user(&state, &reddit_id).await;
            }
            Err(e) => {
                tracing::warn!(reddit_id, "Reddit fetch failed: {e}");
                backoff_user(&state, &reddit_id).await;
            }
        }
    }
}

/// For each tracked subreddit, fetch subscriber/mod status and post/comment counts.
async fn refresh_tracked_subreddits(
    state: &AppState,
    reddit_id: &str,
    username: &str,
    access_token: &str,
) {
    let tracked: Vec<String> = match sqlx::query_scalar::<_, String>(
        "SELECT subreddit FROM tracked_subreddits",
    )
    .fetch_all(&state.pool)
    .await
    {
        Ok(subs) => subs,
        Err(_) => return,
    };

    if tracked.is_empty() {
        return;
    }

    for sub in &tracked {
        // Check subscriber/moderator status
        match state
            .reddit_client
            .check_subreddit_status(access_token, sub)
            .await
        {
            Ok(status) => {
                let _ = sqlx::query(
                    "INSERT INTO user_subreddit_data (reddit_id, subreddit, is_subscriber, is_moderator) \
                     VALUES ($1, $2, $3, $4) \
                     ON CONFLICT (reddit_id, subreddit) DO UPDATE SET \
                     is_subscriber = COALESCE($3, user_subreddit_data.is_subscriber), \
                     is_moderator = COALESCE($4, user_subreddit_data.is_moderator), \
                     fetched_at = now()",
                )
                .bind(reddit_id)
                .bind(sub)
                .bind(status.is_subscriber)
                .bind(status.is_moderator)
                .execute(&state.pool)
                .await;
            }
            Err(RedditError::SubredditInaccessible) => {
                tracing::debug!(reddit_id, sub, "Subreddit inaccessible, skipping");
            }
            Err(e) => {
                tracing::debug!(reddit_id, sub, "Subreddit status check failed: {e}");
            }
        }

        // Count posts and comments in this subreddit (expensive — paginated)
        match state
            .reddit_client
            .count_user_posts(access_token, username, sub)
            .await
        {
            Ok(count) => {
                let _ = sqlx::query(
                    "UPDATE user_subreddit_data SET post_count = $1, fetched_at = now() \
                     WHERE reddit_id = $2 AND subreddit = $3",
                )
                .bind(count)
                .bind(reddit_id)
                .bind(sub)
                .execute(&state.pool)
                .await;
            }
            Err(e) => {
                tracing::debug!(reddit_id, sub, "Post count fetch failed: {e}");
            }
        }

        match state
            .reddit_client
            .count_user_comments(access_token, username, sub)
            .await
        {
            Ok(count) => {
                let _ = sqlx::query(
                    "UPDATE user_subreddit_data SET comment_count = $1, fetched_at = now() \
                     WHERE reddit_id = $2 AND subreddit = $3",
                )
                .bind(count)
                .bind(reddit_id)
                .bind(sub)
                .execute(&state.pool)
                .await;
            }
            Err(e) => {
                tracing::debug!(reddit_id, sub, "Comment count fetch failed: {e}");
            }
        }
    }
}

/// Exponential backoff for a user on failure.
async fn backoff_user(state: &AppState, reddit_id: &str) {
    let _ = sqlx::query(
        "UPDATE user_cache SET fetch_failures = fetch_failures + 1, \
         next_fetch_at = now() + LEAST(INTERVAL '60 seconds' * POWER(2, fetch_failures), INTERVAL '1 hour') \
         WHERE reddit_id = $1",
    )
    .bind(reddit_id)
    .execute(&state.pool)
    .await;
}
