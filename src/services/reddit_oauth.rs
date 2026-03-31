use base64::Engine;

use crate::config::AppConfig;
use crate::error::AppError;

#[derive(serde::Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
}

pub struct RedditOAuth {
    http: reqwest::Client,
}

impl RedditOAuth {
    pub fn with_client(http: reqwest::Client) -> Self {
        Self { http }
    }

    pub fn authorize_url(config: &AppConfig, state: &str) -> String {
        let redirect_uri = config.reddit_redirect_uri();
        format!(
            "https://www.reddit.com/api/v1/authorize?\
             client_id={}&response_type=code&state={}&\
             redirect_uri={}&duration=permanent&\
             scope=identity+read+mysubreddits+history",
            config.reddit_client_id,
            state,
            urlencoding::encode(&redirect_uri),
        )
    }

    /// Exchange authorization code for tokens.
    /// Reddit requires HTTP Basic Auth for token exchange.
    /// Returns (access_token, refresh_token).
    pub async fn exchange_code(
        &self,
        config: &AppConfig,
        code: &str,
    ) -> Result<(String, Option<String>), AppError> {
        let credentials = format!("{}:{}", config.reddit_client_id, config.reddit_client_secret);
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);

        let resp: TokenResponse = self
            .http
            .post("https://www.reddit.com/api/v1/access_token")
            .header("Authorization", format!("Basic {encoded}"))
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", &config.reddit_redirect_uri()),
            ])
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Reddit token exchange failed: {e}")))?
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Reddit token parse failed: {e}")))?;

        Ok((resp.access_token, resp.refresh_token))
    }

    /// Refresh an access token using a stored refresh token.
    /// Reddit refresh tokens do NOT rotate (unlike Discord).
    /// Returns new access_token only.
    pub async fn refresh_access_token(
        &self,
        config: &AppConfig,
        refresh_token: &str,
    ) -> Result<String, AppError> {
        let credentials = format!("{}:{}", config.reddit_client_id, config.reddit_client_secret);
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);

        let resp: TokenResponse = self
            .http
            .post("https://www.reddit.com/api/v1/access_token")
            .header("Authorization", format!("Basic {encoded}"))
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
            ])
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Reddit token refresh failed: {e}")))?
            .json()
            .await
            .map_err(|e| AppError::Internal(format!("Reddit token refresh parse failed: {e}")))?;

        Ok(resp.access_token)
    }
}
