//! TD §6.3's limits: duplicates, rate and a full inbox; you are never limited.

use std::time::Duration;

use super::*;
use crate::tool::{ErrorCode, LimitKind};

fn limit(r: &ToolResult) -> ErrorCode {
    r.as_ref().unwrap_err().code
}

#[test]
fn duplicate_refused_within_10_minutes() {
    let mut h = Harness::new();
    idle(&mut h, "squad-1");
    message(&mut h, "squad-1", "pay-qa", "same").unwrap();
    h.at(Duration::from_secs(9 * 60));
    let again = message(&mut h, "squad-1", "pay-qa", " same ");
    assert_eq!(limit(&again), ErrorCode::Limit(LimitKind::Duplicate));
    h.at(Duration::from_millis(10 * 60 * 1000 + 1));
    message(&mut h, "squad-1", "pay-qa", "same").unwrap();
}

#[test]
fn rate_limited_to_5_a_minute_per_pair() {
    let mut h = Harness::new();
    idle(&mut h, "squad-1");
    for i in 0..5 {
        message(&mut h, "squad-1", "pay-qa", &format!("m{i}")).unwrap();
    }
    let sixth = message(&mut h, "squad-1", "pay-qa", "m5");
    assert_eq!(limit(&sixth), ErrorCode::Limit(LimitKind::Rate));
    message(&mut h, "squad-1", "squad-2", "m5").unwrap();
}

#[test]
fn inbox_full_at_20_waiting() {
    let mut h = Harness::new();
    idle(&mut h, "squad-1");
    // 13 s apart: never 5 within a minute.
    let step = Duration::from_secs(13);
    for i in 0..20 {
        h.at(step * i);
        message(&mut h, "squad-1", "pay-qa", &format!("m{i}")).unwrap();
    }
    h.at(step * 20);
    let full = message(&mut h, "squad-1", "pay-qa", "m20");
    assert_eq!(limit(&full), ErrorCode::Limit(LimitKind::InboxFull));
}

#[test]
fn user_never_throttled() {
    let mut h = Harness::new();
    working(&mut h, "pay-qa");
    for _ in 0..30 {
        let events = you(&mut h, "pay-qa", "same");
        assert_eq!(queued_ids(&events).len(), 1, "{events:?}");
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, Event::MessageRefused { .. })),
            "{events:?}"
        );
    }
    assert_eq!(h.engine.snapshot().agents[&n("pay-qa")].inbox, 30);
}
