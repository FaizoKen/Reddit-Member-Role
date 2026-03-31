use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ConditionField {
    TotalKarma,
    PostKarma,
    CommentKarma,
    SubredditKarma,
    AccountAge,
    EmailVerified,
    IsModerator,
    IsSubscriber,
    HasPremium,
    SubredditPostCount,
    SubredditCommentCount,
}

impl ConditionField {
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            Self::TotalKarma
                | Self::PostKarma
                | Self::CommentKarma
                | Self::SubredditKarma
                | Self::AccountAge
                | Self::SubredditPostCount
                | Self::SubredditCommentCount
        )
    }

    pub fn is_boolean(&self) -> bool {
        matches!(
            self,
            Self::EmailVerified | Self::IsModerator | Self::IsSubscriber | Self::HasPremium
        )
    }

    pub fn requires_subreddit(&self) -> bool {
        matches!(
            self,
            Self::SubredditKarma
                | Self::IsModerator
                | Self::IsSubscriber
                | Self::SubredditPostCount
                | Self::SubredditCommentCount
        )
    }

    pub fn json_key(&self) -> &'static str {
        match self {
            Self::TotalKarma => "totalKarma",
            Self::PostKarma => "postKarma",
            Self::CommentKarma => "commentKarma",
            Self::SubredditKarma => "subredditKarma",
            Self::AccountAge => "accountAge",
            Self::EmailVerified => "emailVerified",
            Self::IsModerator => "isModerator",
            Self::IsSubscriber => "isSubscriber",
            Self::HasPremium => "hasPremium",
            Self::SubredditPostCount => "subredditPostCount",
            Self::SubredditCommentCount => "subredditCommentCount",
        }
    }

    /// Returns the PostgreSQL column name for global fields,
    /// or None for subreddit-specific fields (require JOIN).
    pub fn sql_column(&self) -> Option<&'static str> {
        match self {
            Self::TotalKarma => Some("uc.total_karma"),
            Self::PostKarma => Some("uc.post_karma"),
            Self::CommentKarma => Some("uc.comment_karma"),
            Self::AccountAge => Some("uc.account_age_days"),
            Self::EmailVerified => Some("uc.email_verified"),
            Self::HasPremium => Some("uc.has_premium"),
            Self::SubredditKarma
            | Self::IsModerator
            | Self::IsSubscriber
            | Self::SubredditPostCount
            | Self::SubredditCommentCount => None,
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "totalKarma" => Some(Self::TotalKarma),
            "postKarma" => Some(Self::PostKarma),
            "commentKarma" => Some(Self::CommentKarma),
            "subredditKarma" => Some(Self::SubredditKarma),
            "accountAge" => Some(Self::AccountAge),
            "emailVerified" => Some(Self::EmailVerified),
            "isModerator" => Some(Self::IsModerator),
            "isSubscriber" => Some(Self::IsSubscriber),
            "hasPremium" => Some(Self::HasPremium),
            "subredditPostCount" => Some(Self::SubredditPostCount),
            "subredditCommentCount" => Some(Self::SubredditCommentCount),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ConditionOperator {
    Eq,
    Gt,
    Gte,
    Lt,
    Lte,
    Between,
}

impl ConditionOperator {
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "eq" => Some(Self::Eq),
            "gt" => Some(Self::Gt),
            "gte" => Some(Self::Gte),
            "lt" => Some(Self::Lt),
            "lte" => Some(Self::Lte),
            "between" => Some(Self::Between),
            _ => None,
        }
    }

    pub fn key(&self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Gt => "gt",
            Self::Gte => "gte",
            Self::Lt => "lt",
            Self::Lte => "lte",
            Self::Between => "between",
        }
    }

    pub fn sql_operator(&self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::Gt => ">",
            Self::Gte => ">=",
            Self::Lt => "<",
            Self::Lte => "<=",
            Self::Between => "BETWEEN",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub field: ConditionField,
    pub operator: ConditionOperator,
    pub value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_end: Option<serde_json::Value>,
    /// Subreddit name for subreddit-specific conditions (lowercase, no /r/ prefix)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subreddit: Option<String>,
}
