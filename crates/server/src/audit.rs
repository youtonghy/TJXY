//! Structured authentication audit events (`target = "tjxy_server::audit"`).
//!
//! Callers pass identifiers only. Passwords, tokens, API keys, and raw
//! `Authorization` headers must never be handed to this module.

use std::net::IpAddr;

/// One audit record. `outcome` is `success`, `failure`, or `denied`.
pub(crate) struct Event<'a> {
    pub(crate) name: &'static str,
    pub(crate) outcome: &'static str,
    pub(crate) client_ip: IpAddr,
    /// Authenticated actor (or the normalized login name for login events).
    pub(crate) actor: Option<&'a str>,
    /// Affected account or resource identifier.
    pub(crate) target: Option<&'a str>,
    pub(crate) device_id: Option<&'a str>,
    /// Coarse failure classification such as `invalid_credentials`.
    pub(crate) reason: Option<&'static str>,
    pub(crate) detail: Option<&'a str>,
}

impl<'a> Event<'a> {
    pub(crate) const fn new(name: &'static str, outcome: &'static str, client_ip: IpAddr) -> Self {
        Self {
            name,
            outcome,
            client_ip,
            actor: None,
            target: None,
            device_id: None,
            reason: None,
            detail: None,
        }
    }

    pub(crate) const fn actor(mut self, actor: &'a str) -> Self {
        self.actor = Some(actor);
        self
    }

    pub(crate) const fn target(mut self, target: &'a str) -> Self {
        self.target = Some(target);
        self
    }

    pub(crate) const fn device(mut self, device_id: &'a str) -> Self {
        self.device_id = Some(device_id);
        self
    }

    pub(crate) const fn reason(mut self, reason: &'static str) -> Self {
        self.reason = Some(reason);
        self
    }

    pub(crate) const fn detail(mut self, detail: &'a str) -> Self {
        self.detail = Some(detail);
        self
    }

    pub(crate) fn emit(&self) {
        if self.outcome == "success" {
            tracing::info!(
                target: "tjxy_server::audit",
                event = self.name,
                outcome = self.outcome,
                client_ip = %self.client_ip,
                actor = self.actor,
                subject = self.target,
                device_id = self.device_id,
                reason = self.reason,
                detail = self.detail,
                "authentication audit"
            );
        } else {
            tracing::warn!(
                target: "tjxy_server::audit",
                event = self.name,
                outcome = self.outcome,
                client_ip = %self.client_ip,
                actor = self.actor,
                subject = self.target,
                device_id = self.device_id,
                reason = self.reason,
                detail = self.detail,
                "authentication audit"
            );
        }
    }
}

/// Logs a rate-limit hit for the named action.
pub(crate) fn rate_limited(
    action: &'static str,
    client_ip: IpAddr,
    actor: Option<&str>,
    limited: &crate::login_guard::Limited,
) {
    let detail = format!(
        "scope={} retry_after_secs={}",
        limited.scope.as_str(),
        limited.retry_after_seconds()
    );
    let mut event = Event::new("rate_limited", "denied", client_ip)
        .reason("rate_limited")
        .detail(&detail)
        .target(action);
    if let Some(actor) = actor {
        event = event.actor(actor);
    }
    event.emit();
}
