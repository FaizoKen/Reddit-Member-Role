use std::num::NonZeroU32;
use std::sync::Arc;

use governor::{Quota, RateLimiter};
use serde::Deserialize;

use crate::config::AppConfig;
use crate::error::RedditError;

#[derive(Clone)]
pub struct RedditClient {
    http: reqwest::Client,
    rate_limiter: Arc<RateLimiter<governor::state::NotKeyed, governor::state::InMemoryState, governor::clock::DefaultClock>>,
}

#[derive(Debug, Deserialize)]
pub struct RedditUserData {
    pub name: String,
    pub id: String,
    pub total_karma: i64,
    pub link_karma: i64,
    pub comment_karma: i64,
    pub created_utc: f64,
    pub has_verified_email: bool,
    pub is_gold: bool,
}

#[derive(Debug, Deserialize)]
pub struct SubredditKarma {
    pub sr: String,
    pub comment_karma: i64,
    pub link_karma: i64,
}

#[derive(Debug)]
pub struct SubredditStatus {
    pub is_moderator: Option<bool>,
    pub is_subscriber: Option<bool>,
}

impl RedditClient {
    pub fn new(config: &AppConfig) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(&config.reddit_user_agent)
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to build Reddit HTTP client");

        // 1 request per second (60/min, half of Reddit's limit for safety)
        let quota = Quota::per_second(NonZeroU32::new(1).unwrap());
        let rate_limiter = Arc::new(RateLimiter::direct(quota));

        Self { http, rate_limiter }
    }

    pub async fn wait_for_permit(&self) {
        self.rate_limiter.until_ready().await;
    }

    fn check_status(status: reqwest::StatusCode) -> Result<(), RedditError> {
        match status.as_u16() {
            200..=299 => Ok(()),
            401 => Err(RedditError::TokenRevoked),
            403 => Err(RedditError::Suspended),
            404 => Err(RedditError::NotFound),
            429 => Err(RedditError::RateLimited),
            code => Err(RedditError::Server(code as u16)),
        }
    }

    /// Fetch authenticated user's profile data.
    pub async fn fetch_user_data(
        &self,
        access_token: &str,
    ) -> Result<RedditUserData, RedditError> {
        self.rate_limiter.until_ready().await;

        let resp = self
            .http
            .get("https://oauth.reddit.com/api/v1/me")
            .header("Authorization", format!("Bearer {access_token}"))
            .send()
            .await?;

        Self::check_status(resp.status())?;

        let data: RedditUserData = resp.json().await?;
        Ok(data)
    }

    /// Fetch karma breakdown by subreddit for the authenticated user.
    pub async fn fetch_karma_breakdown(
        &self,
        access_token: &str,
    ) -> Result<Vec<SubredditKarma>, RedditError> {
        self.rate_limiter.until_ready().await;

        let resp = self
            .http
            .get("https://oauth.reddit.com/api/v1/me/karma")
            .header("Authorization", format!("Bearer {access_token}"))
            .send()
            .await?;

        Self::check_status(resp.status())?;

        #[derive(Deserialize)]
        struct KarmaResponse {
            data: Vec<SubredditKarma>,
        }

        let body: KarmaResponse = resp.json().await?;
        Ok(body.data)
    }

    /// Check if the authenticated user is a moderator or subscriber of a subreddit.
    pub async fn check_subreddit_status(
        &self,
        access_token: &str,
        subreddit: &str,
    ) -> Result<SubredditStatus, RedditError> {
        self.rate_limiter.until_ready().await;

        let url = format!("https://oauth.reddit.com/r/{subreddit}/about");
        let resp = self
            .http
            .get(&url)
            .header("Authorization", format!("Bearer {access_token}"))
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::FORBIDDEN
            || resp.status() == reqwest::StatusCode::NOT_FOUND
        {
            return Err(RedditError::SubredditInaccessible);
        }
        Self::check_status(resp.status())?;

        let body: serde_json::Value = resp.json().await?;
        let data = &body["data"];

        Ok(SubredditStatus {
            is_moderator: data["user_is_moderator"].as_bool(),
            is_subscriber: data["user_is_subscriber"].as_bool(),
        })
    }

    /// Count user posts in a specific subreddit (paginated, max 1000).
    pub async fn count_user_posts(
        &self,
        access_token: &str,
        username: &str,
        subreddit: &str,
    ) -> Result<i32, RedditError> {
        self.count_user_items(access_token, username, subreddit, "submitted").await
    }

    /// Count user comments in a specific subreddit (paginated, max 1000).
    pub async fn count_user_comments(
        &self,
        access_token: &str,
        username: &str,
        subreddit: &str,
    ) -> Result<i32, RedditError> {
        self.count_user_items(access_token, username, subreddit, "comments").await
    }

    async fn count_user_items(
        &self,
        access_token: &str,
        username: &str,
        subreddit: &str,
        kind: &str,
    ) -> Result<i32, RedditError> {
        let mut count = 0i32;
        let mut after: Option<String> = None;

        // Paginate up to 10 pages (100 per page = 1000 max)
        for _ in 0..10 {
            self.rate_limiter.until_ready().await;

            let mut url = format!(
                "https://oauth.reddit.com/user/{username}/{kind}?limit=100&sr_detail=false"
            );
            if let Some(ref cursor) = after {
                url.push_str(&format!("&after={cursor}"));
            }

            let resp = self
                .http
                .get(&url)
                .header("Authorization", format!("Bearer {access_token}"))
                .send()
                .await?;

            Self::check_status(resp.status())?;

            let body: serde_json::Value = resp.json().await?;
            let children = body["data"]["children"].as_array();

            let Some(items) = children else {
                break;
            };

            // Filter by subreddit (case-insensitive)
            let sub_lower = subreddit.to_lowercase();
            let matching = items
                .iter()
                .filter(|item| {
                    item["data"]["subreddit"]
                        .as_str()
                        .is_some_and(|s| s.to_lowercase() == sub_lower)
                })
                .count();
            count += matching as i32;

            // Check for next page
            match body["data"]["after"].as_str() {
                Some(next) => after = Some(next.to_string()),
                None => break,
            }
        }

        Ok(count)
    }
}
