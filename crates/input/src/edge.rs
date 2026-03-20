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

#[cfg(test)]
mod tests {
    use super::*;
    use s_kvm_core::PeerId;

    fn make_display(id: u32, x: i32, y: i32, width: u32, height: u32) -> DisplayInfo {
        DisplayInfo {
            id,
            name: format!("Display {id}"),
            x,
            y,
            width,
            height,
            refresh_rate: 60.0,
            scale_factor: 1.0,
            is_primary: id == 0,
        }
    }

    fn make_link(
        source_display: u32,
        source_edge: ScreenEdge,
        target_display: u32,
    ) -> ScreenLink {
        ScreenLink {
            source_display,
            source_edge,
            target_peer: PeerId::new(),
            target_display,
            offset: 0,
        }
    }

    #[test]
    fn within_bounds_center() {
        let displays = vec![make_display(0, 0, 0, 1920, 1080)];
        let detector = EdgeDetector::new(displays, vec![], 0);
        assert!(matches!(
            detector.check(0, 960, 540),
            EdgeCheckResult::WithinBounds
        ));
    }

    #[test]
    fn within_bounds_with_dead_zone() {
        let displays = vec![make_display(0, 0, 0, 1920, 1080)];
        let detector = EdgeDetector::new(displays, vec![], 5);
        // Just inside the dead zone boundary — still within bounds
        assert!(matches!(
            detector.check(0, 100, 100),
            EdgeCheckResult::WithinBounds
        ));
    }

    #[test]
    fn unknown_display_returns_within_bounds() {
        let displays = vec![make_display(0, 0, 0, 1920, 1080)];
        let detector = EdgeDetector::new(displays, vec![], 0);
        assert!(matches!(
            detector.check(99, 0, 0),
            EdgeCheckResult::WithinBounds
        ));
    }

    #[test]
    fn left_edge_unlinked() {
        let displays = vec![make_display(0, 0, 0, 1920, 1080)];
        let detector = EdgeDetector::new(displays, vec![], 0);
        match detector.check(0, 0, 540) {
            EdgeCheckResult::UnlinkedEdge(ScreenEdge::Left) => {}
            other => panic!("Expected UnlinkedEdge(Left), got {:?}", other),
        }
    }

    #[test]
    fn right_edge_unlinked() {
        let displays = vec![make_display(0, 0, 0, 1920, 1080)];
        let detector = EdgeDetector::new(displays, vec![], 0);
        match detector.check(0, 1920, 540) {
            EdgeCheckResult::UnlinkedEdge(ScreenEdge::Right) => {}
            other => panic!("Expected UnlinkedEdge(Right), got {:?}", other),
        }
    }

    #[test]
    fn top_edge_unlinked() {
        let displays = vec![make_display(0, 0, 0, 1920, 1080)];
        let detector = EdgeDetector::new(displays, vec![], 0);
        match detector.check(0, 960, 0) {
            EdgeCheckResult::UnlinkedEdge(ScreenEdge::Top) => {}
            other => panic!("Expected UnlinkedEdge(Top), got {:?}", other),
        }
    }

    #[test]
    fn bottom_edge_unlinked() {
        let displays = vec![make_display(0, 0, 0, 1920, 1080)];
        let detector = EdgeDetector::new(displays, vec![], 0);
        match detector.check(0, 960, 1080) {
            EdgeCheckResult::UnlinkedEdge(ScreenEdge::Bottom) => {}
            other => panic!("Expected UnlinkedEdge(Bottom), got {:?}", other),
        }
    }

    #[test]
    fn right_edge_transition_same_resolution() {
        let displays = vec![
            make_display(0, 0, 0, 1920, 1080),
            make_display(1, 1920, 0, 1920, 1080),
        ];
        let links = vec![make_link(0, ScreenEdge::Right, 1)];
        let detector = EdgeDetector::new(displays, links, 0);

        match detector.check(0, 1920, 540) {
            EdgeCheckResult::Transition {
                target_x, target_y, ..
            } => {
                // Right edge → target left side (x=1920), Y scaled 1:1
                assert_eq!(target_x, 1920); // target.x = 1920
                assert_eq!(target_y, 540);
            }
            other => panic!("Expected Transition, got {:?}", other),
        }
    }

    #[test]
    fn left_edge_transition_same_resolution() {
        let displays = vec![
            make_display(0, 0, 0, 1920, 1080),
            make_display(1, -1920, 0, 1920, 1080),
        ];
        let links = vec![make_link(0, ScreenEdge::Left, 1)];
        let detector = EdgeDetector::new(displays, links, 0);

        match detector.check(0, 0, 540) {
            EdgeCheckResult::Transition {
                target_x, target_y, ..
            } => {
                // Left edge → target right side (target.x + width - 1)
                assert_eq!(target_x, -1920 + 1920 - 1);
                assert_eq!(target_y, 540);
            }
            other => panic!("Expected Transition, got {:?}", other),
        }
    }

    #[test]
    fn coordinate_mapping_different_resolutions() {
        // Source: 1920x1080, Target: 2560x1440
        let displays = vec![
            make_display(0, 0, 0, 1920, 1080),
            make_display(1, 1920, 0, 2560, 1440),
        ];
        let links = vec![make_link(0, ScreenEdge::Right, 1)];
        let detector = EdgeDetector::new(displays, links, 0);

        // Cursor at right edge, halfway down (y=540 / 1080 = 0.5)
        match detector.check(0, 1920, 540) {
            EdgeCheckResult::Transition {
                target_x, target_y, ..
            } => {
                assert_eq!(target_x, 1920); // target.x
                // 540/1080 * 1440 = 720
                assert_eq!(target_y, 720);
            }
            other => panic!("Expected Transition, got {:?}", other),
        }
    }

    #[test]
    fn coordinate_mapping_top_bottom_different_resolutions() {
        // Source: 1920x1080, Target: 3840x2160 (4K)
        let displays = vec![
            make_display(0, 0, 0, 1920, 1080),
            make_display(1, 0, -2160, 3840, 2160),
        ];
        let links = vec![make_link(0, ScreenEdge::Top, 1)];
        let detector = EdgeDetector::new(displays, links, 0);

        // Cursor at top edge, 1/4 across (x=480 / 1920 = 0.25)
        match detector.check(0, 480, 0) {
            EdgeCheckResult::Transition {
                target_x, target_y, ..
            } => {
                // 480/1920 * 3840 = 960
                assert_eq!(target_x, 960);
                // Top edge → target bottom (target.y + height - 1) = -2160 + 2160 - 1 = -1
                assert_eq!(target_y, -1);
            }
            other => panic!("Expected Transition, got {:?}", other),
        }
    }

    #[test]
    fn bottom_edge_transition() {
        let displays = vec![
            make_display(0, 0, 0, 1920, 1080),
            make_display(1, 0, 1080, 1920, 1080),
        ];
        let links = vec![make_link(0, ScreenEdge::Bottom, 1)];
        let detector = EdgeDetector::new(displays, links, 0);

        match detector.check(0, 960, 1080) {
            EdgeCheckResult::Transition {
                target_x, target_y, ..
            } => {
                assert_eq!(target_x, 960);
                assert_eq!(target_y, 1080); // target.y
            }
            other => panic!("Expected Transition, got {:?}", other),
        }
    }

    #[test]
    fn dead_zone_triggers_edge() {
        let displays = vec![make_display(0, 0, 0, 1920, 1080)];
        let detector = EdgeDetector::new(displays, vec![], 10);

        // Position within the dead zone on the left
        match detector.check(0, 5, 540) {
            EdgeCheckResult::UnlinkedEdge(ScreenEdge::Left) => {}
            other => panic!("Expected UnlinkedEdge(Left), got {:?}", other),
        }

        // Position within the dead zone on the right
        match detector.check(0, 1915, 540) {
            EdgeCheckResult::UnlinkedEdge(ScreenEdge::Right) => {}
            other => panic!("Expected UnlinkedEdge(Right), got {:?}", other),
        }
    }

    #[test]
    fn corner_top_left_prefers_left() {
        // When at (0,0), the check evaluates left first
        let displays = vec![make_display(0, 0, 0, 1920, 1080)];
        let detector = EdgeDetector::new(displays, vec![], 0);

        match detector.check(0, 0, 0) {
            EdgeCheckResult::UnlinkedEdge(ScreenEdge::Left) => {}
            other => panic!("Expected UnlinkedEdge(Left) at corner, got {:?}", other),
        }
    }

    #[test]
    fn corner_bottom_right_prefers_right() {
        let displays = vec![make_display(0, 0, 0, 1920, 1080)];
        let detector = EdgeDetector::new(displays, vec![], 0);

        match detector.check(0, 1920, 1080) {
            EdgeCheckResult::UnlinkedEdge(ScreenEdge::Right) => {}
            other => panic!("Expected UnlinkedEdge(Right) at corner, got {:?}", other),
        }
    }

    #[test]
    fn display_with_offset_position() {
        // Display at non-zero origin
        let displays = vec![
            make_display(0, 0, 0, 1920, 1080),
            make_display(1, 1920, 200, 1920, 1080),
        ];
        let links = vec![make_link(1, ScreenEdge::Left, 0)];
        let detector = EdgeDetector::new(displays, links, 0);

        // Cursor at left edge of display 1 (x=1920, y=740 which is 540 relative to display 1)
        match detector.check(1, 1920, 740) {
            EdgeCheckResult::Transition {
                target_x, target_y, ..
            } => {
                // Left edge → target right side (target.x + width - 1 = 0 + 1920 - 1 = 1919)
                assert_eq!(target_x, 1919);
                // relative_y = (740 - 200) / 1080 = 540/1080 = 0.5, target_y = 0 + 0.5 * 1080 = 540
                assert_eq!(target_y, 540);
            }
            other => panic!("Expected Transition, got {:?}", other),
        }
    }

    #[test]
    fn update_displays_changes_detection() {
        let displays = vec![make_display(0, 0, 0, 1920, 1080)];
        let mut detector = EdgeDetector::new(displays, vec![], 0);

        // Initially detects edge
        assert!(matches!(
            detector.check(0, 0, 540),
            EdgeCheckResult::UnlinkedEdge(ScreenEdge::Left)
        ));

        // After updating to a different display, old ID returns WithinBounds
        detector.update_displays(vec![make_display(1, 0, 0, 2560, 1440)]);
        assert!(matches!(
            detector.check(0, 0, 540),
            EdgeCheckResult::WithinBounds
        ));
    }

    #[test]
    fn update_links_adds_transition() {
        let displays = vec![
            make_display(0, 0, 0, 1920, 1080),
            make_display(1, 1920, 0, 1920, 1080),
        ];
        let mut detector = EdgeDetector::new(displays, vec![], 0);

        // Initially unlinked
        assert!(matches!(
            detector.check(0, 1920, 540),
            EdgeCheckResult::UnlinkedEdge(ScreenEdge::Right)
        ));

        // After adding a link, should transition
        detector.update_links(vec![make_link(0, ScreenEdge::Right, 1)]);
        assert!(matches!(
            detector.check(0, 1920, 540),
            EdgeCheckResult::Transition { .. }
        ));
    }
}
