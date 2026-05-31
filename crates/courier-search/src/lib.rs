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
        let filters = parse_query(query);
        let terms = searchable_terms(&filters);

        let mut results = if terms.is_empty() {
            self.storage.list_threads().unwrap_or_default()
        } else {
            self.storage
                .search_threads(&terms.join(" "))
                .unwrap_or_default()
        };

        results.retain(|thread| matches_filters(thread, &filters));
        results
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

fn searchable_terms(filters: &[SearchFilter]) -> Vec<String> {
    filters
        .iter()
        .filter_map(|filter| match filter {
            SearchFilter::Keyword(value)
            | SearchFilter::From(value)
            | SearchFilter::To(value)
            | SearchFilter::Subject(value) => Some(value.clone()),
            SearchFilter::Account(_)
            | SearchFilter::Folder(_)
            | SearchFilter::HasAttachment
            | SearchFilter::IsUnread
            | SearchFilter::IsStarred
            | SearchFilter::Before(_)
            | SearchFilter::After(_) => None,
        })
        .collect()
}

fn matches_filters(thread: &ThreadSummary, filters: &[SearchFilter]) -> bool {
    filters.iter().all(|filter| match filter {
        SearchFilter::Keyword(value) => contains_anywhere(thread, value),
        SearchFilter::From(value) => thread.sender.to_lowercase().contains(&value.to_lowercase()),
        SearchFilter::Subject(value) => thread
            .subject
            .to_lowercase()
            .contains(&value.to_lowercase()),
        SearchFilter::Account(value) => thread.account_id.0 == *value,
        SearchFilter::IsUnread => thread.unread,
        SearchFilter::To(_)
        | SearchFilter::Folder(_)
        | SearchFilter::HasAttachment
        | SearchFilter::IsStarred
        | SearchFilter::Before(_)
        | SearchFilter::After(_) => true,
    })
}

fn contains_anywhere(thread: &ThreadSummary, value: &str) -> bool {
    let value = value.to_lowercase();
    thread.subject.to_lowercase().contains(&value)
        || thread.sender.to_lowercase().contains(&value)
        || thread.snippet.to_lowercase().contains(&value)
}
