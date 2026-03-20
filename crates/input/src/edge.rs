//! Screen edge detection for cursor transitions.

use s_kvm_core::{DisplayInfo, ScreenEdge, ScreenLink};

/// Detects when cursor has hit a screen edge and should transition.
pub struct EdgeDetector {
    displays: Vec<DisplayInfo>,
    links: Vec<ScreenLink>,
    dead_zone: u32,
}

/// Result of edge detection check.
#[derive(Debug)]
pub enum EdgeCheckResult {
    /// Cursor is within screen bounds, no transition.
    WithinBounds,
    /// Cursor has hit a linked edge and should transition.
    Transition {
        link: ScreenLink,
        /// Mapped X coordinate on the target screen.
        target_x: i32,
        /// Mapped Y coordinate on the target screen.
        target_y: i32,
    },
    /// Cursor hit an edge with no link configured.
    UnlinkedEdge(ScreenEdge),
}

impl EdgeDetector {
    pub fn new(displays: Vec<DisplayInfo>, links: Vec<ScreenLink>, dead_zone: u32) -> Self {
        Self {
            displays,
            links,
            dead_zone,
        }
    }

    /// Check if cursor position triggers an edge transition.
    pub fn check(&self, display_id: u32, x: i32, y: i32) -> EdgeCheckResult {
        let display = match self.displays.iter().find(|d| d.id == display_id) {
            Some(d) => d,
            None => return EdgeCheckResult::WithinBounds,
        };

        let dz = self.dead_zone as i32;
        let edge = if x <= display.x + dz {
            Some(ScreenEdge::Left)
        } else if x >= display.x + display.width as i32 - dz {
            Some(ScreenEdge::Right)
        } else if y <= display.y + dz {
            Some(ScreenEdge::Top)
        } else if y >= display.y + display.height as i32 - dz {
            Some(ScreenEdge::Bottom)
        } else {
            None
        };

        match edge {
            Some(edge) => {
                // Find a link for this edge
                if let Some(link) = self.links.iter().find(|l| {
                    l.source_display == display_id && l.source_edge == edge
                }) {
                    // Map coordinates to target display
                    let (target_x, target_y) = self.map_coordinates(
                        display, edge, x, y, link,
                    );
                    EdgeCheckResult::Transition {
                        link: link.clone(),
                        target_x,
                        target_y,
                    }
                } else {
                    EdgeCheckResult::UnlinkedEdge(edge)
                }
            }
            None => EdgeCheckResult::WithinBounds,
        }
    }

    fn map_coordinates(
        &self,
        source: &DisplayInfo,
        edge: ScreenEdge,
        x: i32,
        y: i32,
        link: &ScreenLink,
    ) -> (i32, i32) {
        // Find target display dimensions (default to source if not found)
        let target = self.displays.iter()
            .find(|d| d.id == link.target_display)
            .unwrap_or(source);

        match edge {
            ScreenEdge::Left | ScreenEdge::Right => {
                // Scale Y proportionally
                let relative_y = (y - source.y) as f64 / source.height as f64;
                let target_y = target.y + (relative_y * target.height as f64) as i32;
                let target_x = if edge == ScreenEdge::Left {
                    target.x + target.width as i32 - 1
                } else {
                    target.x
                };
                (target_x, target_y)
            }
            ScreenEdge::Top | ScreenEdge::Bottom => {
                // Scale X proportionally
                let relative_x = (x - source.x) as f64 / source.width as f64;
                let target_x = target.x + (relative_x * target.width as f64) as i32;
                let target_y = if edge == ScreenEdge::Top {
                    target.y + target.height as i32 - 1
                } else {
                    target.y
                };
                (target_x, target_y)
            }
        }
    }

    pub fn update_displays(&mut self, displays: Vec<DisplayInfo>) {
        self.displays = displays;
    }

    pub fn update_links(&mut self, links: Vec<ScreenLink>) {
        self.links = links;
    }
}
