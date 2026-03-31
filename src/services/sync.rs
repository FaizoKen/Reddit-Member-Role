use std::collections::{HashMap, HashSet};

use futures_util::stream::{self, StreamExt};
use sqlx::PgPool;

use crate::error::AppError;
use crate::models::condition::{Condition, ConditionField, ConditionOperator};
use crate::services::condition_eval::{evaluate_conditions, SubredditDataRow, UserCacheRow};
use crate::services::rolelogic::RoleLogicClient;

/// Events sent to the player sync worker (lightweight, per-user).
#[derive(Debug, Clone)]
pub enum PlayerSyncEvent {
    PlayerUpdated { discord_id: String },
    AccountLinked { discord_id: String },
    AccountUnlinked { discord_id: String },
}

/// Events sent to the config sync worker (heavy, per-role-link).
#[derive(Debug, Clone)]
pub struct ConfigSyncEvent {
    pub guild_id: String,
    pub role_id: String,
}

/// Sync roles for a single player across all guilds.
pub async fn sync_for_player(
    discord_id: &str,
    pool: &PgPool,
    rl_client: &RoleLogicClient,
) -> Result<(), AppError> {
    // Get player's cached data
    let cache_row = sqlx::query_as::<_, (i32, i32, i32, i32, bool, bool)>(
        "SELECT uc.total_karma, uc.post_karma, uc.comment_karma, \
         uc.account_age_days, uc.email_verified, uc.has_premium \
         FROM user_cache uc \
         JOIN linked_accounts la ON la.reddit_id = uc.reddit_id \
         WHERE la.discord_id = $1",
    )
    .bind(discord_id)
    .fetch_optional(pool)
    .await?;

    let Some((total_karma, post_karma, comment_karma, account_age_days, email_verified, has_premium)) =
        cache_row
    else {
        return Ok(());
    };

    let cache = UserCacheRow {
        total_karma,
        post_karma,
        comment_karma,
        account_age_days,
        email_verified,
        has_premium,
    };

    // Load subreddit data for this user
    let sub_rows = sqlx::query_as::<_, (String, Option<bool>, Option<bool>, i32, i32, Option<i32>, Option<i32>)>(
        "SELECT usd.subreddit, usd.is_subscriber, usd.is_moderator, \
         usd.post_karma, usd.comment_karma, usd.post_count, usd.comment_count \
         FROM user_subreddit_data usd \
         JOIN linked_accounts la ON la.reddit_id = usd.reddit_id \
         WHERE la.discord_id = $1",
    )
    .bind(discord_id)
    .fetch_all(pool)
    .await?;

    let subreddit_data: HashMap<String, SubredditDataRow> = sub_rows
        .into_iter()
        .map(|(sub, is_sub, is_mod, pk, ck, pc, cc)| {
            (
                sub,
                SubredditDataRow {
                    is_subscriber: is_sub,
                    is_moderator: is_mod,
                    post_karma: pk,
                    comment_karma: ck,
                    post_count: pc,
                    comment_count: cc,
                },
            )
        })
        .collect();

    // Get role links only for guilds this user is a member of
    let role_links = sqlx::query_as::<_, (String, String, String, sqlx::types::Json<Vec<Condition>>)>(
        "SELECT rl.guild_id, rl.role_id, rl.api_token, rl.conditions \
         FROM role_links rl \
         JOIN user_guilds ug ON ug.guild_id = rl.guild_id \
         WHERE ug.discord_id = $1",
    )
    .bind(discord_id)
    .fetch_all(pool)
    .await?;

    // Batch: fetch all existing assignments
    let existing: HashSet<(String, String)> = sqlx::query_as::<_, (String, String)>(
        "SELECT guild_id, role_id FROM role_assignments WHERE discord_id = $1",
    )
    .bind(discord_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();

    // Phase 1: evaluate conditions locally
    enum Action {
        Add {
            guild_id: String,
            role_id: String,
            api_token: String,
        },
        Remove {
            guild_id: String,
            role_id: String,
            api_token: String,
        },
    }

    let mut actions: Vec<Action> = Vec::new();
    for (guild_id, role_id, api_token, conditions) in &role_links {
        let qualifies = evaluate_conditions(conditions, &cache, &subreddit_data);
        let currently_assigned = existing.contains(&(guild_id.clone(), role_id.clone()));
        match (qualifies, currently_assigned) {
            (true, false) => actions.push(Action::Add {
                guild_id: guild_id.clone(),
                role_id: role_id.clone(),
                api_token: api_token.clone(),
            }),
            (false, true) => actions.push(Action::Remove {
                guild_id: guild_id.clone(),
                role_id: role_id.clone(),
                api_token: api_token.clone(),
            }),
            _ => {}
        }
    }

    if actions.is_empty() {
        return Ok(());
    }

    // Phase 2: execute API calls concurrently (max 10 parallel)
    let discord_id_owned = discord_id.to_string();
    stream::iter(actions)
        .for_each_concurrent(10, |action| {
            let pool = pool.clone();
            let rl_client = rl_client.clone();
            let discord_id = discord_id_owned.clone();
            async move {
                match action {
                    Action::Add {
                        guild_id,
                        role_id,
                        api_token,
                    } => {
                        match rl_client
                            .add_user(&guild_id, &role_id, &discord_id, &api_token)
                            .await
                        {
                            Err(AppError::UserLimitReached { limit }) => {
                                tracing::warn!(
                                    guild_id, role_id, discord_id, limit,
                                    "Cannot add user: role link user limit reached"
                                );
                                return;
                            }
                            Err(e) => {
                                tracing::error!(
                                    guild_id, role_id, discord_id,
                                    "Failed to add user to role: {e}"
                                );
                                return;
                            }
                            Ok(_) => {}
                        }
                        if let Err(e) = sqlx::query(
                            "INSERT INTO role_assignments (guild_id, role_id, discord_id) \
                             VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
                        )
                        .bind(&guild_id)
                        .bind(&role_id)
                        .bind(&discord_id)
                        .execute(&pool)
                        .await
                        {
                            tracing::error!(guild_id, role_id, discord_id, "Failed to insert assignment: {e}");
                        }
                    }
                    Action::Remove {
                        guild_id,
                        role_id,
                        api_token,
                    } => {
                        if let Err(e) = rl_client
                            .remove_user(&guild_id, &role_id, &discord_id, &api_token)
                            .await
                        {
                            tracing::error!(
                                guild_id, role_id, discord_id,
                                "Failed to remove user from role: {e}"
                            );
                            return;
                        }
                        if let Err(e) = sqlx::query(
                            "DELETE FROM role_assignments \
                             WHERE guild_id = $1 AND role_id = $2 AND discord_id = $3",
                        )
                        .bind(&guild_id)
                        .bind(&role_id)
                        .bind(&discord_id)
                        .execute(&pool)
                        .await
                        {
                            tracing::error!(guild_id, role_id, discord_id, "Failed to delete assignment: {e}");
                        }
                    }
                }
            }
        })
        .await;

    Ok(())
}

/// Bind value types for dynamic condition queries.
pub(crate) enum ConditionBind {
    Int(i64),
    #[allow(dead_code)]
    Text(String),
    #[allow(dead_code)]
    Bool(bool),
}

/// Build a SQL WHERE clause from conditions for SQL-side filtering.
/// Returns (where_clause, binds, needs_subreddit_join, subreddit_name).
pub fn build_condition_where(
    conditions: &[Condition],
) -> (String, Vec<ConditionBind>, Option<String>) {
    if conditions.is_empty() {
        return ("TRUE".to_string(), vec![], None);
    }

    let mut clauses: Vec<String> = Vec::new();
    let mut binds: Vec<ConditionBind> = Vec::new();
    let mut join_subreddit: Option<String> = None;

    for condition in conditions {
        match &condition.field {
            // Global fields with direct SQL columns
            ConditionField::TotalKarma
            | ConditionField::PostKarma
            | ConditionField::CommentKarma
            | ConditionField::AccountAge => {
                let col = condition.field.sql_column().unwrap();
                let val = condition.value.as_i64().unwrap_or(0);
                if matches!(condition.operator, ConditionOperator::Between) {
                    let end = condition
                        .value_end
                        .as_ref()
                        .and_then(|v| v.as_i64())
                        .unwrap_or(val);
                    let idx_start = binds.len() + 1;
                    let idx_end = binds.len() + 2;
                    clauses.push(format!("{col} >= ${idx_start} AND {col} <= ${idx_end}"));
                    binds.push(ConditionBind::Int(val));
                    binds.push(ConditionBind::Int(end));
                } else {
                    let op = condition.operator.sql_operator();
                    let idx = binds.len() + 1;
                    clauses.push(format!("{col} {op} ${idx}"));
                    binds.push(ConditionBind::Int(val));
                }
            }

            // Boolean global fields
            ConditionField::EmailVerified => {
                clauses.push("uc.email_verified = TRUE".to_string());
            }
            ConditionField::HasPremium => {
                clauses.push("uc.has_premium = TRUE".to_string());
            }

            // Subreddit-specific fields (require JOIN)
            ConditionField::SubredditKarma => {
                join_subreddit = condition.subreddit.clone();
                let val = condition.value.as_i64().unwrap_or(0);
                if matches!(condition.operator, ConditionOperator::Between) {
                    let end = condition
                        .value_end
                        .as_ref()
                        .and_then(|v| v.as_i64())
                        .unwrap_or(val);
                    let idx_start = binds.len() + 1;
                    let idx_end = binds.len() + 2;
                    clauses.push(format!(
                        "(usd.post_karma + usd.comment_karma) >= ${idx_start} AND (usd.post_karma + usd.comment_karma) <= ${idx_end}"
                    ));
                    binds.push(ConditionBind::Int(val));
                    binds.push(ConditionBind::Int(end));
                } else {
                    let op = condition.operator.sql_operator();
                    let idx = binds.len() + 1;
                    clauses.push(format!(
                        "(usd.post_karma + usd.comment_karma) {op} ${idx}"
                    ));
                    binds.push(ConditionBind::Int(val));
                }
            }
            ConditionField::IsModerator => {
                join_subreddit = condition.subreddit.clone();
                clauses.push("usd.is_moderator = TRUE".to_string());
            }
            ConditionField::IsSubscriber => {
                join_subreddit = condition.subreddit.clone();
                clauses.push("usd.is_subscriber = TRUE".to_string());
            }
            ConditionField::SubredditPostCount => {
                join_subreddit = condition.subreddit.clone();
                let val = condition.value.as_i64().unwrap_or(0);
                if matches!(condition.operator, ConditionOperator::Between) {
                    let end = condition
                        .value_end
                        .as_ref()
                        .and_then(|v| v.as_i64())
                        .unwrap_or(val);
                    let idx_start = binds.len() + 1;
                    let idx_end = binds.len() + 2;
                    clauses.push(format!(
                        "COALESCE(usd.post_count, 0) >= ${idx_start} AND COALESCE(usd.post_count, 0) <= ${idx_end}"
                    ));
                    binds.push(ConditionBind::Int(val));
                    binds.push(ConditionBind::Int(end));
                } else {
                    let op = condition.operator.sql_operator();
                    let idx = binds.len() + 1;
                    clauses.push(format!("COALESCE(usd.post_count, 0) {op} ${idx}"));
                    binds.push(ConditionBind::Int(val));
                }
            }
            ConditionField::SubredditCommentCount => {
                join_subreddit = condition.subreddit.clone();
                let val = condition.value.as_i64().unwrap_or(0);
                if matches!(condition.operator, ConditionOperator::Between) {
                    let end = condition
                        .value_end
                        .as_ref()
                        .and_then(|v| v.as_i64())
                        .unwrap_or(val);
                    let idx_start = binds.len() + 1;
                    let idx_end = binds.len() + 2;
                    clauses.push(format!(
                        "COALESCE(usd.comment_count, 0) >= ${idx_start} AND COALESCE(usd.comment_count, 0) <= ${idx_end}"
                    ));
                    binds.push(ConditionBind::Int(val));
                    binds.push(ConditionBind::Int(end));
                } else {
                    let op = condition.operator.sql_operator();
                    let idx = binds.len() + 1;
                    clauses.push(format!("COALESCE(usd.comment_count, 0) {op} ${idx}"));
                    binds.push(ConditionBind::Int(val));
                }
            }
        }
    }

    (clauses.join(" AND "), binds, join_subreddit)
}

/// Re-evaluate all users for a specific role link (after config change).
pub async fn sync_for_role_link(
    guild_id: &str,
    role_id: &str,
    pool: &PgPool,
    rl_client: &RoleLogicClient,
) -> Result<(), AppError> {
    let link = sqlx::query_as::<_, (String, sqlx::types::Json<Vec<Condition>>)>(
        "SELECT api_token, conditions FROM role_links WHERE guild_id = $1 AND role_id = $2",
    )
    .bind(guild_id)
    .bind(role_id)
    .fetch_optional(pool)
    .await?;

    let Some((api_token, conditions)) = link else {
        return Ok(());
    };

    let (_user_count, user_limit) = rl_client
        .get_user_info(guild_id, role_id, &api_token)
        .await
        .unwrap_or((0, 100));

    let (where_clause, binds, join_subreddit) = build_condition_where(&conditions);

    // Build query with optional subreddit JOIN
    let mut bind_idx = binds.len() + 1;
    let sub_join = if join_subreddit.is_some() {
        let sub_idx = bind_idx;
        bind_idx += 1;
        format!(
            "JOIN user_subreddit_data usd ON usd.reddit_id = la.reddit_id AND usd.subreddit = ${sub_idx}"
        )
    } else {
        String::new()
    };

    let guild_bind_idx = bind_idx;
    let limit_bind_idx = bind_idx + 1;

    let query_str = format!(
        "SELECT la.discord_id \
         FROM linked_accounts la \
         JOIN user_cache uc ON uc.reddit_id = la.reddit_id \
         {sub_join} \
         JOIN user_guilds ug ON ug.discord_id = la.discord_id AND ug.guild_id = ${guild_bind_idx} \
         WHERE {where_clause} \
         ORDER BY la.linked_at ASC \
         LIMIT ${limit_bind_idx}",
    );

    // Execute with dynamic binds
    let mut q = sqlx::query_scalar::<_, String>(&query_str);
    for bind in &binds {
        q = match bind {
            ConditionBind::Int(v) => q.bind(*v),
            ConditionBind::Text(v) => q.bind(v),
            ConditionBind::Bool(v) => q.bind(*v),
        };
    }
    if let Some(ref sub) = join_subreddit {
        q = q.bind(sub);
    }
    q = q.bind(guild_id);
    q = q.bind(user_limit as i64);

    let qualifying_ids: Vec<String> = q.fetch_all(pool).await?;

    // Atomic replace
    rl_client
        .replace_users(guild_id, role_id, &qualifying_ids, &api_token)
        .await?;

    // Update local assignments
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM role_assignments WHERE guild_id = $1 AND role_id = $2")
        .bind(guild_id)
        .bind(role_id)
        .execute(&mut *tx)
        .await?;

    if !qualifying_ids.is_empty() {
        sqlx::query(
            "INSERT INTO role_assignments (guild_id, role_id, discord_id) \
             SELECT $1, $2, UNNEST($3::text[])",
        )
        .bind(guild_id)
        .bind(role_id)
        .bind(&qualifying_ids)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    tracing::info!(
        guild_id, role_id,
        qualifying = qualifying_ids.len(),
        "Role link synced"
    );

    Ok(())
}

/// Remove a user from all role assignments (after account unlink).
pub async fn remove_all_assignments(
    discord_id: &str,
    pool: &PgPool,
    rl_client: &RoleLogicClient,
) -> Result<(), AppError> {
    let assignments = sqlx::query_as::<_, (String, String, String)>(
        "SELECT ra.guild_id, ra.role_id, rl.api_token \
         FROM role_assignments ra \
         JOIN role_links rl ON rl.guild_id = ra.guild_id AND rl.role_id = ra.role_id \
         WHERE ra.discord_id = $1",
    )
    .bind(discord_id)
    .fetch_all(pool)
    .await?;

    for (guild_id, role_id, api_token) in &assignments {
        if let Err(e) = rl_client
            .remove_user(guild_id, role_id, discord_id, api_token)
            .await
        {
            tracing::error!(
                guild_id, role_id, discord_id,
                "Failed to remove user during unlink: {e}"
            );
        }
    }

    sqlx::query("DELETE FROM role_assignments WHERE discord_id = $1")
        .bind(discord_id)
        .execute(pool)
        .await?;

    Ok(())
}
