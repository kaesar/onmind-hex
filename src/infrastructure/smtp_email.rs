//! Email → SMTP adapter via lettre (feature `email`).
//!
//! `services.sendEmail(to, subject, body)`. Configured from env:
//! `HEX_SMTP_HOST`, `HEX_SMTP_PORT` (587), `HEX_SMTP_USER`, `HEX_SMTP_PASSWORD`,
//! `HEX_SMTP_FROM`. Uses lettre's blocking `Transport` (./** no Tokio**).

use lettre::message::{header::ContentType, Mailbox, Message};
use lettre::{SmtpTransport, Transport};

use crate::domain::DomainError;

pub struct SmtpEmailSender {
    transport: SmtpTransport,
    from: Mailbox,
}

fn html_body(body: &str) -> String {
    if body.trim_start().starts_with('<') {
        body.to_string()
    } else {
        format!(
            "<p>{}</p>",
            body.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        )
    }
}

fn mailbox(s: &str) -> Result<Mailbox, DomainError> {
    s.parse()
        .map_err(|e| DomainError::Internal(format!("bad mailbox '{s}': {e}")))
}

impl SmtpEmailSender {
    pub fn new_from_env() -> Result<Self, DomainError> {
        let host =
            std::env::var("HEX_SMTP_HOST").map_err(|_| DomainError::Internal("missing HEX_SMTP_HOST".into()))?;
        let port: u16 = std::env::var("HEX_SMTP_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(587);
        let user = std::env::var("HEX_SMTP_USER").unwrap_or_default();
        let pass = std::env::var("HEX_SMTP_PASSWORD").unwrap_or_default();
        let from = mailbox(
            &std::env::var("HEX_SMTP_FROM")
                .map_err(|_| DomainError::Internal("missing HEX_SMTP_FROM".into()))?,
        )?;

        let mut builder = SmtpTransport::relay(&host)
            .map_err(|e| DomainError::Internal(format!("smtp relay {host}: {e}")))?;
        if !user.is_empty() {
            builder = builder.credentials(
                lettre::transport::smtp::authentication::Credentials::new(user, pass),
            );
        }
        let transport = builder.port(port).build();

        Ok(Self { transport, from })
    }
}

impl crate::application::ports::EmailPort for SmtpEmailSender {
    fn send_email_full(
        &self,
        to: &str,
        subject: &str,
        body: &str,
        from: Option<&str>,
        cc: &[String],
    ) -> Result<(), DomainError> {
        let mut builder = Message::builder()
            .from(
                from
                    .filter(|f| !f.is_empty())
                    .map(mailbox)
                    .transpose()?
                    .unwrap_or_else(|| self.from.clone()),
            )
            .to(mailbox(to)?)
            .subject(subject.to_string())
            .header(ContentType::TEXT_HTML);
        for c in cc {
            builder = builder.cc(mailbox(c)?);
        }
        let mail = builder
            .body(html_body(body))
            .map_err(|e| DomainError::Internal(format!("smtp message: {e}")))?;
        self.transport
            .send(&mail)
            .map(|_| ())
            .map_err(|e| DomainError::Internal(format!("smtp send: {e}")))
    }
}