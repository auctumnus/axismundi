use std::sync::{Arc, Mutex};

use askama::Template;
use async_trait::async_trait;
use resend_rs::Resend;
use resend_rs::types::CreateEmailBaseOptions;

use crate::config::{CONFIG, ResendConfig};
use crate::err::AppResult;
use crate::util::urlencode;

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
    client: Resend,
    from_email: String,
}

impl ResendEmailService {
    #[allow(dead_code)]
    pub fn new(config: &ResendConfig) -> Self {
        Self {
            client: Resend::new(&config.api_key),
            from_email: format!("Axismundi <{}>", config.from_email),
        }
    }
}

#[derive(Template)]
#[template(path = "email/verify-email.html")]
struct VerifyEmailHTML<'a> {
    verify_url: &'a str,
}

#[derive(Template)]
#[template(path = "email/verify-email.txt")]
struct VerifyEmailText<'a> {
    verify_url: &'a str,
}

#[async_trait]
impl EmailService for ResendEmailService {
    async fn send_verification_email(
        &self,
        user_id: uuid::Uuid,
        to: &str,
        token: &str,
    ) -> AppResult<()> {
        let verify_url = &CONFIG.url(&format!(
            "verify/{user_id}?token={token}&email={}",
            urlencode(to)
        ));
        let subject = "verify your email";
        let html = VerifyEmailHTML { verify_url }.render().map_err(|e| {
            crate::err::internal_error(format!("failed to render verification email: {}", e))
        })?;
        let text = VerifyEmailText { verify_url }.render().map_err(|e| {
            crate::err::internal_error(format!("failed to render verification email: {}", e))
        })?;

        let email = CreateEmailBaseOptions::new(&self.from_email, [to], subject)
            .with_html(&html)
            .with_text(&text);

        self.client
            .emails
            .send(email)
            .await
            .map_err(|e| crate::err::internal_error(format!("failed to send email: {}", e)))?;

        Ok(())
    }

    async fn send_password_reset_email(
        &self,
        _user_id: uuid::Uuid,
        to: &str,
        token: &str,
    ) -> AppResult<()> {
        let reset_url = &CONFIG.url(&format!("reset-password?token={}", urlencode(token)));
        let subject = "reset your password";
        let html = format!(
            r#"<html>
<body>
<h1>reset your password</h1>
<p>click the link below to reset your password:</p>
<p><a href="{reset_url}">reset password</a></p>
<p>or copy and paste this link into your browser:</p>
<p>{reset_url}</p>
<p>if you didn't request this password reset, you can safely ignore this email.</p>
</body>
</html>"#
        );

        let email = CreateEmailBaseOptions::new(&self.from_email, [to], subject).with_html(&html);

        self.client
            .emails
            .send(email)
            .await
            .map_err(|e| crate::err::internal_error(format!("failed to send email: {}", e)))?;

        Ok(())
    }

    async fn send_email_change_notification(
        &self,
        _user_id: uuid::Uuid,
        old_email: &str,
        new_email: &str,
    ) -> AppResult<()> {
        let subject = "your email address has been changed";
        let html = format!(
            r"<html>
<body>
<h1>email address changed</h1>
<p>this is a notification that your email address has been changed from {old_email} to {new_email}.</p>
<p>if you didn't make this change, please contact support immediately.</p>
</body>
</html>"
        );

        let email_old =
            CreateEmailBaseOptions::new(&self.from_email, [old_email], subject).with_html(&html);
        let email_new =
            CreateEmailBaseOptions::new(&self.from_email, [new_email], subject).with_html(&html);

        self.client.emails.send(email_old).await.map_err(|e| {
            crate::err::internal_error(format!("failed to send email to old address: {}", e))
        })?;

        self.client.emails.send(email_new).await.map_err(|e| {
            crate::err::internal_error(format!("failed to send email to new address: {}", e))
        })?;

        Ok(())
    }
}

#[allow(dead_code)]
pub fn make_email_service(config: &ResendConfig) -> impl EmailService {
    #[cfg(test)]
    {
        let _ = config; // Suppress unused warning in tests
        crate::email::MockEmailService::new()
    }
    #[cfg(not(test))]
    {
        ResendEmailService::new(config)
    }
}

// mock email service for testing
#[derive(Clone, Debug)]
#[allow(dead_code)]
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

    #[allow(dead_code)]
    pub fn get_sent_emails(&self) -> Vec<SentEmail> {
        self.sent_emails.lock().unwrap().clone()
    }

    #[allow(dead_code)]
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
        tracing::debug!("sending verification email; to: {to}, token: {token}, user_id: {user_id}");
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
        tracing::debug!(
            "sending password reset email; to: {to}, token: {token}, user_id: {user_id}"
        );
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
        tracing::debug!(
            "sending email change notification; old_email: {old_email}, new_email: {new_email}, user_id: {user_id}"
        );
        self.sent_emails.lock().unwrap().push(SentEmail {
            to: old_email.to_string(),
            email_type: EmailType::EmailChangeNotification,
            token: String::new(),
            user_id,
        });
        self.sent_emails.lock().unwrap().push(SentEmail {
            to: new_email.to_string(),
            email_type: EmailType::EmailChangeNotification,
            token: String::new(),
            user_id,
        });
        Ok(())
    }
}
