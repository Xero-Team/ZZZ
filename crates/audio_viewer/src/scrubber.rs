use std::time::Duration;

use gpui::{Bounds, Pixels, Point};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScrubKind {
    Seek,
    Volume,
}

pub(crate) struct Scrub {
    pub kind: ScrubKind,
    /// Captured on mouse down so the drag cannot switch to another widget's bounds.
    pub bounds: Bounds<Pixels>,
    /// The displayed seek position, committed only when the drag ends.
    pub pending_seek: Option<Duration>,
}

impl Scrub {
    pub(crate) fn new(kind: ScrubKind, bounds: Bounds<Pixels>) -> Self {
        Self {
            kind,
            bounds,
            pending_seek: None,
        }
    }

    pub(crate) fn update(&mut self, position: Point<Pixels>, duration: Option<Duration>) {
        if self.kind != ScrubKind::Seek {
            return;
        }
        let Some(duration) = duration else {
            return;
        };
        self.pending_seek = Some(position_from_ratio(
            duration,
            ratio_from_position(position, self.bounds),
        ));
    }

    pub(crate) fn pending_position(&self) -> Option<Duration> {
        self.pending_seek
    }
}

pub(crate) fn ratio_from_position(position: Point<Pixels>, bounds: Bounds<Pixels>) -> f32 {
    let x = f32::from(position.x - bounds.origin.x);
    let width = f32::from(bounds.size.width).max(1.0);
    (x / width).clamp(0.0, 1.0)
}

pub(crate) fn position_from_ratio(duration: Duration, ratio: f32) -> Duration {
    Duration::from_secs_f64(duration.as_secs_f64() * ratio.clamp(0.0, 1.0) as f64)
}

#[cfg(test)]
mod tests {
    use super::{Scrub, ScrubKind, position_from_ratio};
    use gpui::{Bounds, point, px, size};
    use std::time::Duration;

    fn scrub() -> Scrub {
        Scrub::new(
            ScrubKind::Seek,
            Bounds {
                origin: point(px(10.0), px(0.0)),
                size: size(px(200.0), px(2.0)),
            },
        )
    }

    #[test]
    fn position_from_ratio_maps_and_clamps() {
        let duration = Duration::from_secs(200);
        assert_eq!(position_from_ratio(duration, 0.0), Duration::ZERO);
        assert_eq!(position_from_ratio(duration, 0.5), Duration::from_secs(100));
        assert_eq!(position_from_ratio(duration, 1.0), duration);
        assert_eq!(position_from_ratio(duration, -0.5), Duration::ZERO);
        assert_eq!(position_from_ratio(duration, 2.0), duration);
    }

    #[test]
    fn pending_position_covers_both_bounds_and_out_of_bounds() {
        let duration = Duration::from_secs(200);
        let mut scrub = scrub();

        scrub.update(point(px(-100.0), px(0.0)), Some(duration));
        assert_eq!(scrub.pending_position(), Some(Duration::ZERO));

        scrub.update(point(px(10.0), px(0.0)), Some(duration));
        assert_eq!(scrub.pending_position(), Some(Duration::ZERO));

        scrub.update(point(px(210.0), px(0.0)), Some(duration));
        assert_eq!(scrub.pending_position(), Some(duration));

        scrub.update(point(px(1000.0), px(0.0)), Some(duration));
        assert_eq!(scrub.pending_position(), Some(duration));
    }

    #[test]
    fn reverse_drag_updates_pending_position() {
        let mut scrub = scrub();
        scrub.update(point(px(210.0), px(0.0)), Some(Duration::from_secs(10)));
        assert_eq!(scrub.pending_position(), Some(Duration::from_secs(10)));

        scrub.update(point(px(110.0), px(0.0)), Some(Duration::from_secs(10)));
        assert_eq!(scrub.pending_position(), Some(Duration::from_secs(5)));
    }

    #[test]
    fn volume_scrub_never_produces_pending_seek() {
        let mut scrub = Scrub::new(
            ScrubKind::Volume,
            Bounds {
                origin: point(px(0.0), px(0.0)),
                size: size(px(100.0), px(2.0)),
            },
        );
        scrub.update(point(px(50.0), px(0.0)), Some(Duration::from_secs(10)));
        assert_eq!(scrub.pending_position(), None);
    }
}
