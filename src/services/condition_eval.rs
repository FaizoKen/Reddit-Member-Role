use std::collections::HashMap;

use crate::models::condition::{Condition, ConditionField, ConditionOperator};

/// Row from user_cache table.
pub struct UserCacheRow {
    pub total_karma: i32,
    pub post_karma: i32,
    pub comment_karma: i32,
    pub account_age_days: i32,
    pub email_verified: bool,
    pub has_premium: bool,
}

/// Row from user_subreddit_data table.
pub struct SubredditDataRow {
    pub is_subscriber: Option<bool>,
    pub is_moderator: Option<bool>,
    pub post_karma: i32,
    pub comment_karma: i32,
    pub post_count: Option<i32>,
    pub comment_count: Option<i32>,
}

/// Evaluate all conditions against user data. All must pass (AND logic).
pub fn evaluate_conditions(
    conditions: &[Condition],
    cache: &UserCacheRow,
    subreddit_data: &HashMap<String, SubredditDataRow>,
) -> bool {
    conditions
        .iter()
        .all(|c| evaluate_single(c, cache, subreddit_data))
}

fn evaluate_single(
    condition: &Condition,
    cache: &UserCacheRow,
    sub_data: &HashMap<String, SubredditDataRow>,
) -> bool {
    match &condition.field {
        ConditionField::TotalKarma => {
            let actual = cache.total_karma as i64;
            let expected = condition.value.as_i64().unwrap_or(0);
            compare(actual, expected, &condition.operator, &condition.value_end)
        }
        ConditionField::PostKarma => {
            let actual = cache.post_karma as i64;
            let expected = condition.value.as_i64().unwrap_or(0);
            compare(actual, expected, &condition.operator, &condition.value_end)
        }
        ConditionField::CommentKarma => {
            let actual = cache.comment_karma as i64;
            let expected = condition.value.as_i64().unwrap_or(0);
            compare(actual, expected, &condition.operator, &condition.value_end)
        }
        ConditionField::AccountAge => {
            let actual = cache.account_age_days as i64;
            let expected = condition.value.as_i64().unwrap_or(0);
            compare(actual, expected, &condition.operator, &condition.value_end)
        }
        ConditionField::EmailVerified => cache.email_verified,
        ConditionField::HasPremium => cache.has_premium,

        ConditionField::SubredditKarma => {
            let sub = condition.subreddit.as_deref().unwrap_or("");
            let sd = sub_data.get(sub);
            let karma = sd.map(|d| (d.post_karma + d.comment_karma) as i64).unwrap_or(0);
            let expected = condition.value.as_i64().unwrap_or(0);
            compare(karma, expected, &condition.operator, &condition.value_end)
        }
        ConditionField::IsModerator => {
            let sub = condition.subreddit.as_deref().unwrap_or("");
            sub_data
                .get(sub)
                .and_then(|d| d.is_moderator)
                .unwrap_or(false)
        }
        ConditionField::IsSubscriber => {
            let sub = condition.subreddit.as_deref().unwrap_or("");
            sub_data
                .get(sub)
                .and_then(|d| d.is_subscriber)
                .unwrap_or(false)
        }
        ConditionField::SubredditPostCount => {
            let sub = condition.subreddit.as_deref().unwrap_or("");
            let count = sub_data
                .get(sub)
                .and_then(|d| d.post_count)
                .unwrap_or(0) as i64;
            let expected = condition.value.as_i64().unwrap_or(0);
            compare(count, expected, &condition.operator, &condition.value_end)
        }
        ConditionField::SubredditCommentCount => {
            let sub = condition.subreddit.as_deref().unwrap_or("");
            let count = sub_data
                .get(sub)
                .and_then(|d| d.comment_count)
                .unwrap_or(0) as i64;
            let expected = condition.value.as_i64().unwrap_or(0);
            compare(count, expected, &condition.operator, &condition.value_end)
        }
    }
}

fn compare(
    actual: i64,
    expected: i64,
    operator: &ConditionOperator,
    value_end: &Option<serde_json::Value>,
) -> bool {
    match operator {
        ConditionOperator::Eq => actual == expected,
        ConditionOperator::Gt => actual > expected,
        ConditionOperator::Gte => actual >= expected,
        ConditionOperator::Lt => actual < expected,
        ConditionOperator::Lte => actual <= expected,
        ConditionOperator::Between => {
            let end = value_end
                .as_ref()
                .and_then(|v| v.as_i64())
                .unwrap_or(expected);
            actual >= expected && actual <= end
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_cache() -> UserCacheRow {
        UserCacheRow {
            total_karma: 5000,
            post_karma: 3000,
            comment_karma: 2000,
            account_age_days: 730,
            email_verified: true,
            has_premium: false,
        }
    }

    #[test]
    fn test_total_karma_gte() {
        let conditions = vec![Condition {
            field: ConditionField::TotalKarma,
            operator: ConditionOperator::Gte,
            value: json!(1000),
            value_end: None,
            subreddit: None,
        }];
        assert!(evaluate_conditions(&conditions, &sample_cache(), &HashMap::new()));
    }

    #[test]
    fn test_total_karma_gte_fail() {
        let conditions = vec![Condition {
            field: ConditionField::TotalKarma,
            operator: ConditionOperator::Gte,
            value: json!(10000),
            value_end: None,
            subreddit: None,
        }];
        assert!(!evaluate_conditions(&conditions, &sample_cache(), &HashMap::new()));
    }

    #[test]
    fn test_email_verified() {
        let conditions = vec![Condition {
            field: ConditionField::EmailVerified,
            operator: ConditionOperator::Eq,
            value: json!(true),
            value_end: None,
            subreddit: None,
        }];
        assert!(evaluate_conditions(&conditions, &sample_cache(), &HashMap::new()));
    }

    #[test]
    fn test_has_premium_false() {
        let conditions = vec![Condition {
            field: ConditionField::HasPremium,
            operator: ConditionOperator::Eq,
            value: json!(true),
            value_end: None,
            subreddit: None,
        }];
        assert!(!evaluate_conditions(&conditions, &sample_cache(), &HashMap::new()));
    }

    #[test]
    fn test_subreddit_moderator() {
        let conditions = vec![Condition {
            field: ConditionField::IsModerator,
            operator: ConditionOperator::Eq,
            value: json!(true),
            value_end: None,
            subreddit: Some("gaming".to_string()),
        }];
        let mut sub_data = HashMap::new();
        sub_data.insert("gaming".to_string(), SubredditDataRow {
            is_subscriber: Some(true),
            is_moderator: Some(true),
            post_karma: 100,
            comment_karma: 200,
            post_count: Some(10),
            comment_count: Some(50),
        });
        assert!(evaluate_conditions(&conditions, &sample_cache(), &sub_data));
    }

    #[test]
    fn test_subreddit_karma() {
        let conditions = vec![Condition {
            field: ConditionField::SubredditKarma,
            operator: ConditionOperator::Gte,
            value: json!(200),
            value_end: None,
            subreddit: Some("rust".to_string()),
        }];
        let mut sub_data = HashMap::new();
        sub_data.insert("rust".to_string(), SubredditDataRow {
            is_subscriber: Some(true),
            is_moderator: Some(false),
            post_karma: 100,
            comment_karma: 150,
            post_count: None,
            comment_count: None,
        });
        assert!(evaluate_conditions(&conditions, &sample_cache(), &sub_data));
    }

    #[test]
    fn test_between_account_age() {
        let conditions = vec![Condition {
            field: ConditionField::AccountAge,
            operator: ConditionOperator::Between,
            value: json!(365),
            value_end: Some(json!(1095)),
            subreddit: None,
        }];
        assert!(evaluate_conditions(&conditions, &sample_cache(), &HashMap::new()));
    }

    #[test]
    fn test_empty_conditions() {
        let conditions: Vec<Condition> = vec![];
        assert!(evaluate_conditions(&conditions, &sample_cache(), &HashMap::new()));
    }
}
