use std::collections::HashMap;

use serde_json::{json, Value};

use crate::error::AppError;
use crate::models::condition::{Condition, ConditionField, ConditionOperator};

pub fn build_config_schema(conditions: &[Condition], verify_url: &str) -> Value {
    let c = conditions.first();

    let mut values = HashMap::new();
    values.insert(
        "field".to_string(),
        json!(c.map(|c| c.field.json_key()).unwrap_or("")),
    );
    values.insert(
        "operator".to_string(),
        json!(c.map(|c| c.operator.key()).unwrap_or("")),
    );

    if let Some(c) = c {
        // Subreddit field
        if let Some(sub) = &c.subreddit {
            values.insert("subreddit".to_string(), json!(sub));
        }

        // Value field (keyed by field type)
        let value_key = format!("value_{}", c.field.json_key());
        if c.field.is_numeric() {
            let val = match &c.value {
                Value::Number(n) => json!(n),
                _ => json!(""),
            };
            values.insert(value_key, val);

            // End value for Between
            if c.operator == ConditionOperator::Between {
                if let Some(end) = &c.value_end {
                    let end_key = format!("value_end_{}", c.field.json_key());
                    let end_val = match end {
                        Value::Number(n) => json!(n),
                        _ => json!(""),
                    };
                    values.insert(end_key, end_val);
                }
            }
        }
    }

    json!({
        "version": 1,
        "name": "Reddit Member Role",
        "description": "Assign Discord roles based on Reddit account data.",
        "sections": [
            {
                "title": "Getting Started",
                "fields": [
                    {
                        "type": "display",
                        "key": "info",
                        "label": "How it works",
                        "value": format!(
                            "This plugin assigns a Discord role based on Reddit account stats.\n\n\
                            Step 1: Members link their Reddit account at:\n{verify_url}\n\n\
                            Step 2: Configure a condition below.\n\n\
                            Step 3: Members who meet the condition get this role automatically.\n\
                            Data is refreshed periodically."
                        )
                    }
                ]
            },
            {
                "title": "Role Condition",
                "description": "Set the Reddit requirement for this role.",
                "fields": [
                    {
                        "type": "select",
                        "key": "field",
                        "label": "Reddit stat",
                        "validation": { "required": true },
                        "options": [
                            {"label": "Total Karma", "value": "totalKarma"},
                            {"label": "Post Karma", "value": "postKarma"},
                            {"label": "Comment Karma", "value": "commentKarma"},
                            {"label": "Subreddit Karma", "value": "subredditKarma"},
                            {"label": "Account Age (days)", "value": "accountAge"},
                            {"label": "Email Verified", "value": "emailVerified"},
                            {"label": "Subreddit Moderator", "value": "isModerator"},
                            {"label": "Subreddit Subscriber", "value": "isSubscriber"},
                            {"label": "Reddit Premium", "value": "hasPremium"},
                            {"label": "Posts in Subreddit", "value": "subredditPostCount"},
                            {"label": "Comments in Subreddit", "value": "subredditCommentCount"}
                        ]
                    },
                    {
                        "type": "text",
                        "key": "subreddit",
                        "label": "Subreddit name",
                        "description": "Without /r/ prefix (e.g. \"gaming\")",
                        "validation": {
                            "required": true,
                            "pattern": "^[A-Za-z0-9_]{2,21}$",
                            "pattern_message": "Must be a valid subreddit name (2-21 chars, letters/numbers/underscores)"
                        },
                        "condition": {
                            "field": "field",
                            "equals_any": ["subredditKarma", "isModerator", "isSubscriber", "subredditPostCount", "subredditCommentCount"]
                        }
                    },
                    {
                        "type": "select",
                        "key": "operator",
                        "label": "Comparison",
                        "default_value": "gte",
                        "condition": {
                            "field": "field",
                            "equals_any": ["totalKarma", "postKarma", "commentKarma", "subredditKarma", "accountAge", "subredditPostCount", "subredditCommentCount"]
                        },
                        "options": [
                            {"label": "= equals", "value": "eq"},
                            {"label": "> greater than", "value": "gt"},
                            {"label": ">= at least", "value": "gte"},
                            {"label": "< less than", "value": "lt"},
                            {"label": "<= at most", "value": "lte"},
                            {"label": "between (range)", "value": "between"}
                        ]
                    },
                    // Value inputs for each numeric field
                    {
                        "type": "number",
                        "key": "value_totalKarma",
                        "label": "Karma amount",
                        "validation": { "required": true, "min": 0 },
                        "condition": { "field": "field", "equals": "totalKarma" }
                    },
                    {
                        "type": "number",
                        "key": "value_end_totalKarma",
                        "label": "Karma amount (end of range)",
                        "validation": { "min": 0 },
                        "pair_with": "value_totalKarma",
                        "conditions": [
                            { "field": "field", "equals": "totalKarma" },
                            { "field": "operator", "equals": "between" }
                        ]
                    },
                    {
                        "type": "number",
                        "key": "value_postKarma",
                        "label": "Post karma amount",
                        "validation": { "required": true, "min": 0 },
                        "condition": { "field": "field", "equals": "postKarma" }
                    },
                    {
                        "type": "number",
                        "key": "value_end_postKarma",
                        "label": "Post karma (end of range)",
                        "validation": { "min": 0 },
                        "pair_with": "value_postKarma",
                        "conditions": [
                            { "field": "field", "equals": "postKarma" },
                            { "field": "operator", "equals": "between" }
                        ]
                    },
                    {
                        "type": "number",
                        "key": "value_commentKarma",
                        "label": "Comment karma amount",
                        "validation": { "required": true, "min": 0 },
                        "condition": { "field": "field", "equals": "commentKarma" }
                    },
                    {
                        "type": "number",
                        "key": "value_end_commentKarma",
                        "label": "Comment karma (end of range)",
                        "validation": { "min": 0 },
                        "pair_with": "value_commentKarma",
                        "conditions": [
                            { "field": "field", "equals": "commentKarma" },
                            { "field": "operator", "equals": "between" }
                        ]
                    },
                    {
                        "type": "number",
                        "key": "value_subredditKarma",
                        "label": "Subreddit karma amount",
                        "validation": { "required": true, "min": 0 },
                        "condition": { "field": "field", "equals": "subredditKarma" }
                    },
                    {
                        "type": "number",
                        "key": "value_end_subredditKarma",
                        "label": "Subreddit karma (end of range)",
                        "validation": { "min": 0 },
                        "pair_with": "value_subredditKarma",
                        "conditions": [
                            { "field": "field", "equals": "subredditKarma" },
                            { "field": "operator", "equals": "between" }
                        ]
                    },
                    {
                        "type": "number",
                        "key": "value_accountAge",
                        "label": "Account age (days)",
                        "validation": { "required": true, "min": 0 },
                        "condition": { "field": "field", "equals": "accountAge" }
                    },
                    {
                        "type": "number",
                        "key": "value_end_accountAge",
                        "label": "Account age (end of range, days)",
                        "validation": { "min": 0 },
                        "pair_with": "value_accountAge",
                        "conditions": [
                            { "field": "field", "equals": "accountAge" },
                            { "field": "operator", "equals": "between" }
                        ]
                    },
                    {
                        "type": "number",
                        "key": "value_subredditPostCount",
                        "label": "Number of posts",
                        "description": "Max trackable: 1000 (Reddit API limit)",
                        "validation": { "required": true, "min": 0, "max": 1000 },
                        "condition": { "field": "field", "equals": "subredditPostCount" }
                    },
                    {
                        "type": "number",
                        "key": "value_end_subredditPostCount",
                        "label": "Posts (end of range)",
                        "validation": { "min": 0, "max": 1000 },
                        "pair_with": "value_subredditPostCount",
                        "conditions": [
                            { "field": "field", "equals": "subredditPostCount" },
                            { "field": "operator", "equals": "between" }
                        ]
                    },
                    {
                        "type": "number",
                        "key": "value_subredditCommentCount",
                        "label": "Number of comments",
                        "description": "Max trackable: 1000 (Reddit API limit)",
                        "validation": { "required": true, "min": 0, "max": 1000 },
                        "condition": { "field": "field", "equals": "subredditCommentCount" }
                    },
                    {
                        "type": "number",
                        "key": "value_end_subredditCommentCount",
                        "label": "Comments (end of range)",
                        "validation": { "min": 0, "max": 1000 },
                        "pair_with": "value_subredditCommentCount",
                        "conditions": [
                            { "field": "field", "equals": "subredditCommentCount" },
                            { "field": "operator", "equals": "between" }
                        ]
                    },
                    // Hint for boolean fields
                    {
                        "type": "display",
                        "key": "bool_hint",
                        "label": "Condition",
                        "value": "Selecting this stat means the user must have it. For example, selecting \"Email Verified\" means only users with a verified email get the role.",
                        "condition": {
                            "field": "field",
                            "equals_any": ["emailVerified", "hasPremium", "isModerator", "isSubscriber"]
                        }
                    }
                ]
            },
            {
                "title": "Examples",
                "collapsible": true,
                "default_collapsed": true,
                "fields": [
                    {
                        "type": "display",
                        "key": "examples",
                        "label": "Common setups",
                        "value": "Total Karma >= 1000  ->  Accounts with 1000+ karma\nAccount Age >= 365  ->  Accounts older than 1 year\nSubreddit Subscriber + gaming  ->  r/gaming members\nSubreddit Moderator + MyServer  ->  Mods of r/MyServer\nPosts in Subreddit >= 5  ->  Active posters\nEmail Verified  ->  Only verified-email accounts\nReddit Premium  ->  Premium subscribers"
                    }
                ]
            }
        ],
        "values": values
    })
}

pub fn parse_config(config: &HashMap<String, Value>) -> Result<Vec<Condition>, AppError> {
    let field_key = config
        .get("field")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("Missing 'field' selection".into()))?;

    let field = ConditionField::from_key(field_key)
        .ok_or_else(|| AppError::BadRequest(format!("Unknown field: {field_key}")))?;

    // Subreddit validation
    let subreddit = if field.requires_subreddit() {
        let sub = config
            .get("subreddit")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::BadRequest("Subreddit name is required for this condition".into()))?;
        let sub = sub.trim().to_lowercase();
        if sub.len() < 2 || sub.len() > 21 {
            return Err(AppError::BadRequest(
                "Subreddit name must be 2-21 characters".into(),
            ));
        }
        if !sub.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(AppError::BadRequest(
                "Subreddit name must contain only letters, numbers, and underscores".into(),
            ));
        }
        Some(sub)
    } else {
        None
    };

    // Boolean fields: implicit operator=Eq, value=true
    if field.is_boolean() {
        return Ok(vec![Condition {
            field,
            operator: ConditionOperator::Eq,
            value: json!(true),
            value_end: None,
            subreddit,
        }]);
    }

    // Numeric fields: parse operator and value
    let operator_key = config
        .get("operator")
        .and_then(|v| v.as_str())
        .unwrap_or("gte");
    let operator = ConditionOperator::from_key(operator_key)
        .ok_or_else(|| AppError::BadRequest(format!("Unknown operator: {operator_key}")))?;

    let value_key = format!("value_{field_key}");
    let value = config
        .get(&value_key)
        .cloned()
        .unwrap_or(Value::Null);

    if !value.is_number() {
        return Err(AppError::BadRequest(format!(
            "A numeric value is required for '{value_key}'"
        )));
    }

    let value_end = if operator == ConditionOperator::Between {
        let end_key = format!("value_end_{field_key}");
        let end = config.get(&end_key).cloned().unwrap_or(Value::Null);
        if !end.is_number() {
            return Err(AppError::BadRequest(format!(
                "End value is required for 'between' operator ('{end_key}')"
            )));
        }
        if let (Some(start), Some(end_val)) = (value.as_i64(), end.as_i64()) {
            if start > end_val {
                return Err(AppError::BadRequest(
                    "Start value must be less than or equal to end value".into(),
                ));
            }
        }
        Some(end)
    } else {
        None
    };

    Ok(vec![Condition {
        field,
        operator,
        value,
        value_end,
        subreddit,
    }])
}
