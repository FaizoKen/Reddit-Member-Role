use std::env;

#[derive(Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub reddit_client_id: String,
    pub reddit_client_secret: String,
    pub reddit_user_agent: String,
    pub session_secret: String,
    pub base_url: String,
    pub listen_addr: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            database_url: env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
            reddit_client_id: env::var("REDDIT_CLIENT_ID")
                .expect("REDDIT_CLIENT_ID must be set"),
            reddit_client_secret: env::var("REDDIT_CLIENT_SECRET")
                .expect("REDDIT_CLIENT_SECRET must be set"),
            reddit_user_agent: env::var("REDDIT_USER_AGENT")
                .unwrap_or_else(|_| "RedditMemberRole/1.0".to_string()),
            session_secret: env::var("SESSION_SECRET").expect("SESSION_SECRET must be set"),
            base_url: env::var("BASE_URL").expect("BASE_URL must be set"),
            listen_addr: env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
        }
    }

    pub fn reddit_redirect_uri(&self) -> String {
        format!("{}/verify/callback", self.base_url)
    }
}
