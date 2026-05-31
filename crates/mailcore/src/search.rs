use mailproto::ThreadSummary;

pub struct SearchIndex;

impl SearchIndex {
    pub async fn query(&self, _query: &str) -> Vec<ThreadSummary> {
        Vec::new()
    }
}
