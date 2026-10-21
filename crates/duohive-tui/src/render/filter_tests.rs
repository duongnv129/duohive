//! The filter chip in the header (UI §4.1.1).

use crate::filter::Filter;
use crate::fixtures::{ctx, render_lines, spec_model};

#[test]
fn filter_chip_snapshots() {
    let mut m = spec_model();
    m.filter = Filter::NeedYou;
    for (w, h) in [(100, 30), (80, 24)] {
        let text = render_lines(&m, &ctx(), w, h).join("\n");
        insta::assert_snapshot!(format!("filter_chip_{w}x{h}"), text);
    }
}

#[test]
fn chip_is_long_when_wide_and_short_when_narrow() {
    let mut m = spec_model();
    m.filter = Filter::NeedYou;
    let wide = render_lines(&m, &ctx(), 100, 30);
    assert!(wide[0].contains("filtered: ◆ · esc clears"), "{}", wide[0]);
    let narrow = render_lines(&m, &ctx(), 80, 24);
    assert!(narrow[0].contains("filter ◆"), "{}", narrow[0]);
    assert!(
        narrow[0].contains("? help"),
        "counts and help stay: {}",
        narrow[0]
    );
}
