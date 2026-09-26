//! Display rotation metadata and the matching pixel buffer transform.

/// A clockwise rotation that must be applied to decoded frames for display.
///
/// Video containers rarely store pixels in display orientation. Instead they
/// carry a display matrix (or transform) that tells the player how far to
/// rotate the decoded image. Both decoder backends resolve that into this type
/// so frames leave the crate already upright.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rotation {
    /// Pixels are already in display orientation.
    #[default]
    None,
    /// Rotate the decoded image 90 degrees clockwise.
    Clockwise90,
    /// Rotate the decoded image 180 degrees.
    Clockwise180,
    /// Rotate the decoded image 270 degrees clockwise.
    Clockwise270,
}

impl Rotation {
    /// Resolve a rotation from an angle in degrees.
    ///
    /// Angles are snapped to the nearest quarter turn, which matches how
    /// players handle the (usually exact) angles found in display matrices.
    pub fn from_degrees(degrees: i32) -> Self {
        match degrees.rem_euclid(360) {
            45..=134 => Self::Clockwise90,
            135..=224 => Self::Clockwise180,
            225..=314 => Self::Clockwise270,
            _ => Self::None,
        }
    }

    /// The rotation as a clockwise angle in degrees.
    pub fn degrees(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Clockwise90 => 90,
            Self::Clockwise180 => 180,
            Self::Clockwise270 => 270,
        }
    }

    /// Whether applying this rotation exchanges a frame's width and height.
    pub fn swaps_dimensions(self) -> bool {
        matches!(self, Self::Clockwise90 | Self::Clockwise270)
    }

    /// Rotate a tightly packed BGRA buffer, returning the new size and pixels.
    ///
    /// [`Rotation::None`] returns the input unchanged and does not copy.
    pub fn apply(self, width: u32, height: u32, bgra: Vec<u8>) -> (u32, u32, Vec<u8>) {
        if self == Self::None {
            return (width, height, bgra);
        }
        rotate_bgra_quarter(&bgra, width, height, self.degrees() / 90)
    }
}

fn rotate_bgra_quarter(
    source: &[u8],
    width: u32,
    height: u32,
    quarter_turns: u32,
) -> (u32, u32, Vec<u8>) {
    let (width, height) = (width as usize, height as usize);
    let (rotated_width, rotated_height) = if quarter_turns == 2 {
        (width, height)
    } else {
        (height, width)
    };
    let mut rotated = vec![0u8; width * height * 4];

    for y in 0..height {
        for x in 0..width {
            let (destination_x, destination_y) = match quarter_turns {
                1 => (height - 1 - y, x),
                2 => (width - 1 - x, height - 1 - y),
                _ => (y, width - 1 - x),
            };
            let source_index = (y * width + x) * 4;
            let destination_index = (destination_y * rotated_width + destination_x) * 4;
            rotated[destination_index..destination_index + 4]
                .copy_from_slice(&source[source_index..source_index + 4]);
        }
    }

    (rotated_width as u32, rotated_height as u32, rotated)
}

#[cfg(test)]
mod tests {
    use super::Rotation;

    #[test]
    fn from_degrees_snaps_to_quarter_turns() {
        assert_eq!(Rotation::from_degrees(0), Rotation::None);
        assert_eq!(Rotation::from_degrees(90), Rotation::Clockwise90);
        assert_eq!(Rotation::from_degrees(180), Rotation::Clockwise180);
        assert_eq!(Rotation::from_degrees(270), Rotation::Clockwise270);
        assert_eq!(Rotation::from_degrees(359), Rotation::None);
        assert_eq!(Rotation::from_degrees(-90), Rotation::Clockwise270);
        assert_eq!(Rotation::from_degrees(30), Rotation::None);
    }

    #[test]
    fn apply_rotates_pixels_clockwise() {
        // A 2x1 image with a single red pixel on the left.
        let red = [0u8, 0, 255, 255];
        let blue = [255u8, 0, 0, 255];
        let source = [red, blue].concat();

        let (width, height, rotated) = Rotation::Clockwise90.apply(2, 1, source);
        assert_eq!((width, height), (1, 2));
        // The left pixel moves to the top of the taller image.
        assert_eq!(&rotated[0..4], &red);
        assert_eq!(&rotated[4..8], &blue);

        let source = [red, blue].concat();
        let (width, height, rotated) = Rotation::Clockwise270.apply(2, 1, source);
        assert_eq!((width, height), (1, 2));
        // The left pixel moves to the bottom of the taller image.
        assert_eq!(&rotated[0..4], &blue);
        assert_eq!(&rotated[4..8], &red);
    }

    #[test]
    fn apply_rotates_180_in_place_dimensions() {
        let red = [0u8, 0, 255, 255];
        let blue = [255u8, 0, 0, 255];
        let source = [red, blue].concat();

        let (width, height, rotated) = Rotation::Clockwise180.apply(2, 1, source);
        assert_eq!((width, height), (2, 1));
        assert_eq!(&rotated[0..4], &blue);
        assert_eq!(&rotated[4..8], &red);
    }

    #[test]
    fn apply_without_rotation_keeps_buffer() {
        let source = vec![0u8, 1, 2, 3];
        let (width, height, rotated) = Rotation::None.apply(1, 1, source.clone());
        assert_eq!((width, height), (1, 1));
        assert_eq!(rotated, source);
    }
}
