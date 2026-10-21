//! The Move to… picker's frame (UI §4.4.2).

use duohive_core::org::NodeKey;
use duohive_core::testkit::n;

use crate::fixtures::{ctx, model_for, render_lines};
use crate::forms::MoveTo;
use crate::input::spec_move_org;
use crate::model::Dialog;

#[test]
fn move_to_snapshots() {
    let org = spec_move_org();
    let mut picker = MoveTo::open(&org, &NodeKey::Agent(n("pay-qa"))).unwrap();
    let mut m = model_for(org);
    // On `Generalist`, the place the spec's picture highlights.
    while picker.selected().is_none_or(|o| o.label != "Generalist") {
        picker.step(true);
    }
    m.dialog = Some(Dialog::MoveTo(Box::new(picker)));
    for (w, h) in [(100, 30), (80, 24)] {
        let text = render_lines(&m, &ctx(), w, h).join("\n");
        insta::assert_snapshot!(format!("move_to_{w}x{h}"), text);
    }
}
