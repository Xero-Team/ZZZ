#[derive(Clone, Copy)]
pub(super) struct DeterminateProgress {
    #[cfg(feature = "accessibility")]
    pub current_value: f64,
    #[cfg(feature = "accessibility")]
    pub max_value: f64,
    pub fraction: f32,
    pub exceeds_maximum: bool,
}

pub(super) fn determinate_progress(value: f32, max_value: f32) -> Option<DeterminateProgress> {
    if !value.is_finite() || !max_value.is_finite() || max_value <= 0. {
        return None;
    }

    let clamped_value = value.clamp(0., max_value);
    Some(DeterminateProgress {
        #[cfg(feature = "accessibility")]
        current_value: f64::from(clamped_value),
        #[cfg(feature = "accessibility")]
        max_value: f64::from(max_value),
        fraction: clamped_value / max_value,
        exceeds_maximum: value > max_value,
    })
}

mod circular_progress;
mod progress_bar;

pub use circular_progress::*;
pub use progress_bar::*;
