//! The bench's stand-in for the TUI: applies every engine event to a
//! `UiModel`, draws it into a `TestBackend` at most 30 times a second, and
//! times each tapped hook against the first frame that shows its state.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use duohive_core::event::Event;
use duohive_core::hook::HookMessage;
use duohive_core::names::AgentName;
use duohive_core::org::Org;
use duohive_core::settings::Settings;
use duohive_core::status::AgentState;
use duohive_core::time::Millis;
use duohive_tui::{Palette, RenderCtx, UiModel, render};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

const COLUMNS: u16 = 160;
const ROWS: u16 = 48;
/// At most 30 frames a second.
pub const FRAME_INTERVAL: Duration = Duration::from_millis(33);

/// The state a hook is expected to put its agent in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wants {
    Working,
    AtRest,
}

impl Wants {
    /// The state a hook leads to, for the hooks the bench times.
    fn of(event: &str) -> Option<Self> {
        match event {
            "busy" => Some(Self::Working),
            "turn-end" => Some(Self::AtRest),
            _ => None,
        }
    }

    fn holds(self, state: AgentState) -> bool {
        match self {
            Self::Working => state == AgentState::Working,
            Self::AtRest => state.is_at_rest(),
        }
    }
}

struct Pending {
    agent: AgentName,
    wants: Wants,
    sent_ms: f64,
}

/// Wall-clock Unix time in fractional milliseconds (the hooks' `sent_at_ms` clock).
fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

/// The model, its frames and the hook timings.
pub struct Observer {
    model: UiModel,
    terminal: Terminal<TestBackend>,
    ctx: RenderCtx,
    dirty: bool,
    last_frame: Option<Instant>,
    /// Each agent's state in the last frame, and when that frame was drawn.
    shown: HashMap<AgentName, (AgentState, f64)>,
    pending: Vec<Pending>,
    samples: Vec<f64>,
}

impl Observer {
    /// An observer over `org`, drawing at 160×48 on socket `tmux_socket`.
    pub fn new(org: Org, tmux_socket: &str) -> Self {
        let Ok(terminal) = Terminal::new(TestBackend::new(COLUMNS, ROWS));
        Self {
            model: UiModel::new(org),
            terminal,
            ctx: RenderCtx {
                now: Millis(0),
                spinner: 0,
                palette: Palette::dark(),
                settings: Settings::default(),
                tmux_socket: tmux_socket.to_owned(),
            },
            dirty: false,
            last_frame: None,
            shown: HashMap::new(),
            pending: Vec::new(),
            samples: Vec::new(),
        }
    }

    /// Applies one engine event; the next frame shows it.
    pub fn event(&mut self, event: Event) {
        self.dirty |= self.model.apply(event);
    }

    /// Notes a hook the engine is about to act on.
    pub fn hook(&mut self, msg: &HookMessage) {
        let (Some(wants), Ok(agent)) = (Wants::of(&msg.event), AgentName::parse(&msg.agent)) else {
            return;
        };
        let sent_ms = msg.sent_at_ms as f64;
        // A newer hook supersedes an older one: the next change is the newer one's.
        self.pending.retain(|p| p.agent != agent);
        match self.shown.get(&agent) {
            // The screen already showed this state: something else caused it (a
            // Give work, a screen read), so the hook's own latency is unknown.
            // It counts only when the frame came after the hook was sent.
            Some((state, at)) if wants.holds(*state) => {
                if *at >= sent_ms {
                    self.samples.push(at - sent_ms);
                }
            }
            _ => self.pending.push(Pending {
                agent,
                wants,
                sent_ms,
            }),
        }
    }

    /// When the next frame is due, if the screen is out of date.
    pub fn next_frame_at(&self) -> Option<Instant> {
        self.dirty.then(|| {
            self.last_frame
                .map_or_else(Instant::now, |t| t + FRAME_INTERVAL)
        })
    }

    /// Draws a frame if one is due.
    pub fn draw_if_due(&mut self) {
        if self.next_frame_at().is_some_and(|at| at <= Instant::now()) {
            self.draw();
        }
    }

    fn draw(&mut self) {
        self.ctx.now = Millis(now_ms() as u64);
        self.ctx.spinner = self.ctx.spinner.wrapping_add(1);
        self.model.now = self.ctx.now;
        let (model, ctx) = (&self.model, &self.ctx);
        // A TestBackend cannot fail to draw; a failure would only lose a frame.
        let _ = self.terminal.draw(|frame| render(frame, model, ctx));
        // The state is on screen once the frame is complete.
        let wall = now_ms();
        self.dirty = false;
        self.last_frame = Some(Instant::now());
        let mut changed = Vec::new();
        for (agent, view) in &self.model.agents {
            let state = view.status.state;
            if self.shown.get(agent).is_none_or(|(old, _)| *old != state) {
                self.shown.insert(agent.clone(), (state, wall));
                changed.push((agent.clone(), state));
            }
        }
        self.resolve(&changed, wall);
    }

    /// Settles pending hooks against the frame that just changed their agents'
    /// states. A pending hook's state was not on screen when it arrived, so the
    /// first change after it is its own: it counts when it is the state the hook
    /// leads to, and is dropped either way (only the first change is the hook's).
    fn resolve(&mut self, changed: &[(AgentName, AgentState)], at: f64) {
        let samples = &mut self.samples;
        self.pending.retain(|p| {
            let Some((_, state)) = changed.iter().find(|(a, _)| *a == p.agent) else {
                return true;
            };
            if p.wants.holds(*state) && at >= p.sent_ms {
                samples.push(at - p.sent_ms);
            }
            false
        });
    }

    /// Hooks still waiting for their agent's next change (tests).
    #[cfg(test)]
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// The model as last applied.
    pub fn model(&self) -> &UiModel {
        &self.model
    }

    /// Hook-to-screen samples so far, in ms.
    pub fn samples(&self) -> &[f64] {
        &self.samples
    }
}
