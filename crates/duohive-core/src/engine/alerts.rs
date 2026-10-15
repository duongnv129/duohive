//! Desktop alert triggers (TD §4.2, UI §4.1.4; P2-D14).

use super::Engine;
use crate::alert::{AlertKind, alert_for};
use crate::event::Event;
use crate::names::AgentName;
use crate::status::Status;

impl Engine {
    /// Raises the alert `agent`'s move from `before` to its current status calls for:
    /// none while start-up replays history (P2-D14) or for a kind turned off.
    pub(super) fn alert_after(&mut self, agent: &AgentName, before: &Status, out: &mut Vec<Event>) {
        if self.replaying {
            return;
        }
        let is_duo = self.org.is_duo(agent);
        let rt = self.rt_mut(agent);
        let Some(alert) = alert_for(agent, is_duo, before, &rt.status, rt.last_reply.as_deref())
        else {
            return;
        };
        let on = match alert.kind {
            AlertKind::NeedsYou => self.settings.alerts.needs_you,
            AlertKind::Finished => self.settings.alerts.finished,
        };
        if on {
            self.ports.notifier.notify(&alert);
            out.push(Event::Alert(alert));
        }
    }

    /// `AlertsUnavailable` for a session whose notifier can't show alerts.
    pub(super) fn alerts_unavailable(&self, out: &mut Vec<Event>) {
        if !self.ports.notifier.available() {
            out.push(Event::AlertsUnavailable);
        }
    }
}

#[cfg(test)]
mod tests;
