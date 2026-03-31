CREATE INDEX IF NOT EXISTS idx_role_links_api_token ON role_links (api_token);
CREATE INDEX IF NOT EXISTS idx_role_assignments_discord_id ON role_assignments (discord_id);
CREATE INDEX IF NOT EXISTS idx_user_cache_next_fetch ON user_cache (next_fetch_at ASC);
CREATE INDEX IF NOT EXISTS idx_user_cache_total_karma ON user_cache (total_karma);
CREATE INDEX IF NOT EXISTS idx_user_cache_account_age ON user_cache (account_age_days);
CREATE INDEX IF NOT EXISTS idx_user_guilds_guild ON user_guilds (guild_id);
CREATE INDEX IF NOT EXISTS idx_user_subreddit_data_sub ON user_subreddit_data (subreddit);
CREATE INDEX IF NOT EXISTS idx_linked_accounts_reddit ON linked_accounts (reddit_id);
CREATE INDEX IF NOT EXISTS idx_discord_tokens_guilds ON discord_tokens (guilds_refreshed_at ASC);
