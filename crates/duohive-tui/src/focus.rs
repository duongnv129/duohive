//! Which pane has focus, and where each pane goes on screen (UI §3, P2-D3, P2-D19).

use ratatui::layout::Rect;

/// A pane of the main screen, in `tab` order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pane {
    /// The org tree.
    #[default]
    Org,
    /// Needs you, pinned at the bottom of the org pane; always a `tab` stop.
    NeedsYou,
    /// The selected node's pane.
    Agent,
    /// The chat with Duo.
    Chat,
}

/// The agent pane's two modes (UI §4.1.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AgentMode {
    /// DuoHive's keys work.
    #[default]
    Selected,
    /// Every key goes to the agent until `ctrl+q`.
    Typing,
}

/// Where keys go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Focus {
    /// The focused pane.
    pub pane: Pane,
    /// The agent pane's mode.
    pub agent_mode: AgentMode,
    /// The highlighted Needs you row.
    pub needs_you_cursor: usize,
}

impl Focus {
    /// The pane `tab` moves to: org → Needs you → agent → chat → org.
    pub fn next(&self) -> Pane {
        match self.pane {
            Pane::Org => Pane::NeedsYou,
            Pane::NeedsYou => Pane::Agent,
            Pane::Agent => Pane::Chat,
            Pane::Chat => Pane::Org,
        }
    }

    /// The pane `shift+tab` moves to.
    pub fn prev(&self) -> Pane {
        match self.pane {
            Pane::Org => Pane::Chat,
            Pane::NeedsYou => Pane::Org,
            Pane::Agent => Pane::NeedsYou,
            Pane::Chat => Pane::Agent,
        }
    }
}

/// Where each pane goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// The org pane's tree part: its top border and the tree rows.
    pub org: Rect,
    /// Needs you: the separator, its rows and the org pane's bottom border.
    pub needs_you: Rect,
    /// The agent pane.
    pub agent: Rect,
    /// The chat pane.
    pub chat: Rect,
    /// The 80–99 × 24 row: compact tree, compact header and footer.
    pub compact: bool,
}

impl Layout {
    /// Needs you shrunk to `count` rows (at most its share); the tree takes the rest.
    pub fn with_needs_you(self, count: usize) -> Self {
        let share = self.needs_you.height.saturating_sub(NEEDS_YOU_FRAME);
        let rows = u16::try_from(count).unwrap_or(u16::MAX).min(share);
        let height = rows + NEEDS_YOU_FRAME;
        let bottom = self.needs_you.bottom();
        Self {
            org: Rect {
                height: bottom - height - self.org.y,
                ..self.org
            },
            needs_you: Rect {
                y: bottom - height,
                height,
                ..self.needs_you
            },
            ..self
        }
    }
}

/// The rows of UI §3's layout table, smallest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Size {
    /// 80–99 × 24.
    Compact,
    /// 100–119 × 30.
    Medium,
    /// ≥ 120 × 36.
    Large,
}

/// Header and footer rows.
const CHROME_ROWS: u16 = 2;
/// The fewest rows the agent pane keeps when the chat grows (P2-D3).
const MIN_AGENT_ROWS: u16 = 6;
/// Needs you's separator and the org pane's bottom border.
const NEEDS_YOU_FRAME: u16 = 2;
/// Needs you takes at most a third of the org pane's inside.
const NEEDS_YOU_SHARE: u16 = 3;
/// The org pane's borders.
const BORDER_ROWS: u16 = 2;

impl Size {
    /// The row for `area`: width and height each pick one, and the smaller applies (P2-D19).
    fn of(area: Rect) -> Option<Self> {
        let by_width = match area.width {
            120.. => Self::Large,
            100..120 => Self::Medium,
            80..100 => Self::Compact,
            _ => return None,
        };
        let by_height = match area.height {
            36.. => Self::Large,
            30..36 => Self::Medium,
            24..30 => Self::Compact,
            _ => return None,
        };
        Some(by_width.min(by_height))
    }

    fn org_width(self) -> u16 {
        match self {
            Self::Large | Self::Medium => 50,
            Self::Compact => 32,
        }
    }

    /// The chat's rows (borders included), before `ctrl+↑`.
    fn chat_rows(self) -> u16 {
        match self {
            Self::Large | Self::Medium => 6,
            Self::Compact => 4,
        }
    }

    /// The chat's rows while typing: only the largest row grows.
    fn typing_rows(self) -> u16 {
        match self {
            Self::Large => 12,
            Self::Medium | Self::Compact => self.chat_rows(),
        }
    }
}

/// Whether the tree shows last replies at `area`: only at ≥ 120×36 (UI §3's shrink order).
pub fn shows_replies(area: Rect) -> bool {
    Size::of(area) == Some(Size::Large)
}

/// The panes for a terminal of `area`, or `None` below 80×24.
pub fn layout(area: Rect, chat_rows_extra: i16, chat_typing: bool) -> Option<Layout> {
    layout_below(area, 0, chat_rows_extra, chat_typing)
}

/// [`layout`] with `banners` rows taken above the header.
pub(crate) fn layout_below(
    area: Rect,
    banners: u16,
    chat_rows_extra: i16,
    chat_typing: bool,
) -> Option<Layout> {
    let size = Size::of(area)?;
    let top = area.y + banners + 1;
    let avail = available_rows(area, banners);
    let (base, most) = chat_bounds(size, avail);
    let extra = u16::try_from(chat_rows_extra.max(0)).unwrap_or(0);
    let wanted = base.saturating_add(extra);
    let wanted = if chat_typing {
        wanted.max(size.typing_rows())
    } else {
        wanted
    };
    let chat_h = wanted.min(most).min(avail);
    let upper = avail - chat_h;
    let org_w = size.org_width().min(area.width);
    let inside = upper.saturating_sub(BORDER_ROWS);
    let ny_h = ((inside / NEEDS_YOU_SHARE).max(1) + NEEDS_YOU_FRAME).min(upper);
    Some(Layout {
        org: Rect::new(area.x, top, org_w, upper - ny_h),
        needs_you: Rect::new(area.x, top + upper - ny_h, org_w, ny_h),
        agent: Rect::new(area.x + org_w, top, area.width - org_w, upper),
        chat: Rect::new(area.x, top + upper, area.width, chat_h),
        compact: size == Size::Compact,
    })
}

/// The most rows `ctrl+↑` can add to the chat at `area` (P2-D3): up to where the agent
/// pane keeps 6 rows.
pub fn chat_extra_max(area: Rect) -> i16 {
    chat_extra_max_below(area, 0)
}

/// [`chat_extra_max`] with `banners` rows taken above the header, as [`layout_below`] counts
/// them.
pub(crate) fn chat_extra_max_below(area: Rect, banners: u16) -> i16 {
    let Some(size) = Size::of(area) else {
        return 0;
    };
    let (base, most) = chat_bounds(size, available_rows(area, banners));
    i16::try_from(most - base).unwrap_or(i16::MAX)
}

/// The rows between the header and the footer.
fn available_rows(area: Rect, banners: u16) -> u16 {
    area.height.saturating_sub(banners + CHROME_ROWS)
}

/// The chat's rows before `ctrl+↑`, and the most it may take: what leaves the agent pane
/// 6 rows, never less than the base.
fn chat_bounds(size: Size, avail: u16) -> (u16, u16) {
    let base = size.chat_rows();
    (base, avail.saturating_sub(MIN_AGENT_ROWS).max(base))
}

#[cfg(test)]
mod tests;
