//! The right-click menu's frame (UI §4.4.2).

use duohive_core::org::NodeKey;
use duohive_core::testkit::n;
use ratatui::layout::{Position, Rect};

use crate::fixtures::{ctx, model_for, render_lines};
use crate::input::{move_spec_org, open_menu_for};

#[test]
fn menu_snapshots() {
    for (w, h) in [(100, 30), (80, 24)] {
        let mut m = model_for(move_spec_org());
        m.selected = NodeKey::Agent(n("pay-qa"));
        m.screen = Rect::new(0, 0, w, h);
        open_menu_for(&mut m, NodeKey::Agent(n("pay-qa")), Position::new(20, 6));
        let text = render_lines(&m, &ctx(), w, h).join("\n");
        insta::assert_snapshot!(format!("menu_{w}x{h}"), text);
    }
}
