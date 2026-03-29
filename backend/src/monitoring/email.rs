use lettre::message::{header::ContentType, Mailbox};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::error::AppError;
use crate::types::{AlertEvent, SmtpConfig};

fn build_transport(config: &SmtpConfig) -> Result<AsyncSmtpTransport<Tokio1Executor>, AppError> {
    let creds = Credentials::new(config.username.clone(), config.password.clone());

    let transport = if config.tls {
        if config.port == 465 {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
                .map_err(|e| AppError::Internal(format!("SMTP relay error: {}", e)))?
                .port(config.port)
                .credentials(creds)
                .build()
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
                .map_err(|e| AppError::Internal(format!("SMTP STARTTLS error: {}", e)))?
                .port(config.port)
                .credentials(creds)
                .build()
        }
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host)
            .port(config.port)
            .credentials(creds)
            .build()
    };

    Ok(transport)
}

fn parse_mailbox(addr: &str) -> Result<Mailbox, AppError> {
    addr.parse::<Mailbox>()
        .map_err(|e| AppError::BadRequest(format!("Invalid email address '{}': {}", addr, e)))
}

pub async fn send_test_email(config: &SmtpConfig, recipient: &str) -> Result<(), AppError> {
    let from = parse_mailbox(&config.from_address)?;
    let to = parse_mailbox(recipient)?;

    let email = Message::builder()
        .from(from)
        .to(to)
        .subject("AnyServer — Test Email")
        .header(ContentType::TEXT_HTML)
        .body(
            "<h2>AnyServer Email Test</h2>\
             <p>If you're reading this, your SMTP configuration is working correctly.</p>\
             <p style=\"color: #888; font-size: 0.9em;\">This is an automated test message from AnyServer.</p>"
                .to_string(),
        )
        .map_err(|e| AppError::Internal(format!("Failed to build test email: {}", e)))?;

    let transport = build_transport(config)?;
    transport
        .send(email)
        .await
        .map_err(|e| AppError::Internal(format!("SMTP send failed: {}", e)))?;

    Ok(())
}

pub async fn send_alert_email(
    config: &SmtpConfig,
    recipients: &[String],
    event: &AlertEvent,
    base_url: Option<&str>,
) -> Result<(), AppError> {
    if recipients.is_empty() {
        return Ok(());
    }

    let from = parse_mailbox(&config.from_address)?;

    let subject = format!(
        "AnyServer Alert: {} — {}",
        event.kind.display_name(),
        event.server_name
    );

    let server_link = base_url
        .map(|url| {
            let url = url.trim_end_matches('/');
            format!(
                "<p><a href=\"{}/servers/{}\" style=\"color: #4a9eff;\">View Server Dashboard →</a></p>",
                url, event.server_id
            )
        })
        .unwrap_or_default();

    let timestamp = event.timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string();

    let body = format!(
        r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #1a1a2e; color: #e0e0e0; padding: 20px;">
  <div style="max-width: 600px; margin: 0 auto; background: #16213e; border-radius: 8px; padding: 24px; border: 1px solid #2a2a4a;">
    <h2 style="margin-top: 0; color: #ff6b6b;">{emoji} {kind}</h2>
    <table style="width: 100%; border-collapse: collapse; margin: 16px 0;">
      <tr>
        <td style="padding: 8px 12px; color: #888; width: 120px;">Server</td>
        <td style="padding: 8px 12px; font-weight: bold;">{server_name}</td>
      </tr>
      <tr>
        <td style="padding: 8px 12px; color: #888;">Event</td>
        <td style="padding: 8px 12px;">{kind}</td>
      </tr>
      <tr>
        <td style="padding: 8px 12px; color: #888;">Time</td>
        <td style="padding: 8px 12px;">{timestamp}</td>
      </tr>
      <tr>
        <td style="padding: 8px 12px; color: #888;">Details</td>
        <td style="padding: 8px 12px;">{message}</td>
      </tr>
    </table>
    {server_link}
    <hr style="border: none; border-top: 1px solid #2a2a4a; margin: 16px 0;">
    <p style="color: #666; font-size: 0.85em; margin-bottom: 0;">
      This alert was sent by AnyServer. You can configure alert settings in the Admin Panel.
    </p>
  </div>
</body>
</html>"#,
        emoji = event.kind.emoji(),
        kind = event.kind.display_name(),
        server_name = html_escape(&event.server_name),
        timestamp = timestamp,
        message = html_escape(&event.message),
        server_link = server_link,
    );

    let transport = build_transport(config)?;

    for recipient in recipients {
        let to = match parse_mailbox(recipient) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Skipping invalid alert recipient '{}': {}", recipient, e);
                continue;
            }
        };

        let email = Message::builder()
            .from(from.clone())
            .to(to)
            .subject(&subject)
            .header(ContentType::TEXT_HTML)
            .body(body.clone())
            .map_err(|e| AppError::Internal(format!("Failed to build alert email: {}", e)))?;

        if let Err(e) = transport.send(email).await {
            tracing::error!("Failed to send alert email to '{}': {}", recipient, e);
        }
    }

    Ok(())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_escape() {
        assert_eq!(
            html_escape("<script>alert('xss')</script>"),
            "&lt;script&gt;alert('xss')&lt;/script&gt;"
        );
        assert_eq!(html_escape("normal text"), "normal text");
        assert_eq!(html_escape("a & b \"c\""), "a &amp; b &quot;c&quot;");
    }

    #[test]
    fn test_parse_mailbox_valid() {
        let result = parse_mailbox("test@example.com");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_mailbox_invalid() {
        let result = parse_mailbox("not-an-email");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_mailbox_with_name() {
        let result = parse_mailbox("AnyServer <alerts@example.com>");
        assert!(result.is_ok());
    }
}
