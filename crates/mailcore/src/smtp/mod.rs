use mailproto::DraftMessage;

pub struct SmtpSender;

impl SmtpSender {
    pub async fn send(&self, _draft: DraftMessage) -> crate::Result<()> {
        Ok(())
    }
}
