use async_trait::async_trait;

use crate::err::AppResult;

#[async_trait]
pub trait EmailService: Send + Sync + std::fmt::Debug {
    async fn send_verification_email(
        &self,
        user_id: uuid::Uuid,
        to: &str,
        token: &str,
    ) -> AppResult<()>;

    async fn send_password_reset_email(
        &self,
        user_id: uuid::Uuid,
        to: &str,
        token: &str,
    ) -> AppResult<()>;

    async fn send_email_change_notification(
        &self,
        user_id: uuid::Uuid,
        old_email: &str,
        new_email: &str,
    ) -> AppResult<()>;
}

#[derive(Debug, Clone)]
pub struct ResendEmailService {
    // TODO: add smtp config
}

impl ResendEmailService {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl EmailService for ResendEmailService {
    async fn send_verification_email(
        &self,
        user_id: uuid::Uuid,
        to: &str,
        token: &str,
    ) -> AppResult<()> {
        // TODO: implement actual email sending
        println!("would send verification email to {to} (user {user_id}) with token {token}");
        Ok(())
    }

    async fn send_password_reset_email(
        &self,
        user_id: uuid::Uuid,
        to: &str,
        token: &str,
    ) -> AppResult<()> {
        // TODO: implement actual email sending
        println!("would send password reset email to {to} (user {user_id}) with token {token}");
        Ok(())
    }

    async fn send_email_change_notification(
        &self,
        user_id: uuid::Uuid,
        old_email: &str,
        new_email: &str,
    ) -> AppResult<()> {
        // TODO: implement actual email sending
        println!(
            "would send email change notification to {old_email} and {new_email} (user {user_id})"
        );
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    use std::sync::{Arc, Mutex};

    // mock email service for testing
    #[derive(Clone, Debug)]
    pub struct SentEmail {
        pub to: String,
        pub email_type: EmailType,
        pub token: String,
        pub user_id: uuid::Uuid,
    }

    #[derive(Clone, Debug, PartialEq)]
    pub enum EmailType {
        Verification,
        PasswordReset,
        EmailChangeNotification,
    }

    #[derive(Debug, Clone)]
    pub struct MockEmailService {
        pub sent_emails: Arc<Mutex<Vec<SentEmail>>>,
    }

    impl MockEmailService {
        pub fn new() -> Self {
            Self {
                sent_emails: Arc::new(Mutex::new(Vec::new())),
            }
        }

        pub fn get_sent_emails(&self) -> Vec<SentEmail> {
            self.sent_emails.lock().unwrap().clone()
        }

        pub fn clear(&self) {
            self.sent_emails.lock().unwrap().clear();
        }
    }

    #[async_trait]
    impl EmailService for MockEmailService {
        async fn send_verification_email(
            &self,
            user_id: uuid::Uuid,
            to: &str,
            token: &str,
        ) -> AppResult<()> {
            self.sent_emails.lock().unwrap().push(SentEmail {
                to: to.to_string(),
                email_type: EmailType::Verification,
                token: token.to_string(),
                user_id,
            });
            Ok(())
        }

        async fn send_password_reset_email(
            &self,
            user_id: uuid::Uuid,
            to: &str,
            token: &str,
        ) -> AppResult<()> {
            self.sent_emails.lock().unwrap().push(SentEmail {
                to: to.to_string(),
                email_type: EmailType::PasswordReset,
                token: token.to_string(),
                user_id,
            });
            Ok(())
        }

        async fn send_email_change_notification(
            &self,
            user_id: uuid::Uuid,
            old_email: &str,
            new_email: &str,
        ) -> AppResult<()> {
            self.sent_emails.lock().unwrap().push(SentEmail {
                to: old_email.to_string(),
                email_type: EmailType::EmailChangeNotification,
                token: "".to_string(),
                user_id,
            });
            self.sent_emails.lock().unwrap().push(SentEmail {
                to: new_email.to_string(),
                email_type: EmailType::EmailChangeNotification,
                token: "".to_string(),
                user_id,
            });
            Ok(())
        }
    }
}
