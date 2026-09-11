//! Status marks: a small dot plus a restrained text label.

use agentifi_domain::{AgentSession, AttachmentState};

/// `attached · active` header summary for the session workspace.
#[must_use]
pub fn header_summary(session: &AgentSession, attachment: AttachmentState) -> String {
    format!("{} · {}", attachment.label(), session.status.label())
}
