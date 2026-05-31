use courier_proto::ThreadSummary;
use courier_storage::Storage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchFilter {
    From(String),
    To(String),
    Subject(String),
    Account(String),
    Folder(String),
    HasAttachment,
    IsUnread,
    IsStarred,
    Before(String),
    After(String),
    Keyword(String),
}

#[derive(Debug, Clone)]
pub struct SearchIndex {
    storage: Storage,
}

impl SearchIndex {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    pub async fn query(&self, query: &str) -> Vec<ThreadSummary> {
        let _db_path = self.storage.db_path();
        let _filters = parse_query(query);
        Vec::new()
    }
}

pub fn parse_query(query: &str) -> Vec<SearchFilter> {
    query
        .split_whitespace()
        .map(|token| {
            if let Some(value) = token.strip_prefix("from:") {
                SearchFilter::From(value.to_string())
            } else if let Some(value) = token.strip_prefix("to:") {
                SearchFilter::To(value.to_string())
            } else if let Some(value) = token.strip_prefix("subject:") {
                SearchFilter::Subject(value.to_string())
            } else if let Some(value) = token.strip_prefix("account:") {
                SearchFilter::Account(value.to_string())
            } else if let Some(value) = token.strip_prefix("folder:") {
                SearchFilter::Folder(value.to_string())
            } else if token == "has:attachment" {
                SearchFilter::HasAttachment
            } else if token == "is:unread" {
                SearchFilter::IsUnread
            } else if token == "is:starred" {
                SearchFilter::IsStarred
            } else if let Some(value) = token.strip_prefix("before:") {
                SearchFilter::Before(value.to_string())
            } else if let Some(value) = token.strip_prefix("after:") {
                SearchFilter::After(value.to_string())
            } else {
                SearchFilter::Keyword(token.to_string())
            }
        })
        .collect()
}
