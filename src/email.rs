use async_trait::async_trait;
use std::sync::{Arc, Mutex};

#[async_trait]
pub trait EmailService: Send + Sync {
    async fn send_verification_email(
        &self,
        to: &str,
        token: &str,
    ) -> Result<(), Box<dyn std::error::Error>>;

    async fn send_password_reset_email(
        &self,
        to: &str,
        token: &str,
    ) -> Result<(), Box<dyn std::error::Error>>;
}

// real email service (placeholder for now - add smtp/ses/sendgrid later)
pub struct SmtpEmailService {
    // TODO: add smtp config
}

impl SmtpEmailService {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl EmailService for SmtpEmailService {
    async fn send_verification_email(
        &self,
        to: &str,
        token: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // TODO: implement actual email sending
        println!("would send verification email to {} with token {}", to, token);
        Ok(())
    }

    async fn send_password_reset_email(
        &self,
        to: &str,
        token: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // TODO: implement actual email sending
        println!("would send password reset email to {} with token {}", to, token);
        Ok(())
    }
}

// mock email service for testing
#[derive(Clone, Debug)]
pub struct SentEmail {
    pub to: String,
    pub email_type: EmailType,
    pub token: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EmailType {
    Verification,
    PasswordReset,
}

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
        to: &str,
        token: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.sent_emails.lock().unwrap().push(SentEmail {
            to: to.to_string(),
            email_type: EmailType::Verification,
            token: token.to_string(),
        });
        Ok(())
    }

    async fn send_password_reset_email(
        &self,
        to: &str,
        token: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.sent_emails.lock().unwrap().push(SentEmail {
            to: to.to_string(),
            email_type: EmailType::PasswordReset,
            token: token.to_string(),
        });
        Ok(())
    }
}
