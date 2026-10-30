use std::iter::FromIterator;

/// Bits reserved per letter, enough to store a saturated count of three.
const BITS_PER_LETTER: u8 = 2;
/// Offset of the digit slots: 26 letters, each using [`BITS_PER_LETTER`] bits.
const DIGIT_BIT_OFFSET: u8 = 52;
/// Bit marking that the bag contains a hyphen.
const HYPHEN_BIT_INDEX: u8 = 62;

pub fn simple_lowercase(character: char) -> char {
    character.to_lowercase().next().unwrap_or(character)
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct CharBag(u64);

impl CharBag {
    pub fn is_superset(self, other: CharBag) -> bool {
        self.0 & other.0 == other.0
    }

    fn insert(&mut self, character: char) {
        let character = simple_lowercase(character);
        if character.is_ascii_lowercase() {
            let index = character as u8 - b'a';
            let mut count = self.0;
            count >>= index * BITS_PER_LETTER;
            count = ((count << 1) | 1) & 3;
            count <<= index * BITS_PER_LETTER;
            self.0 |= count;
        } else if character.is_ascii_digit() {
            let index = character as u8 - b'0';
            self.0 |= 1 << (index + DIGIT_BIT_OFFSET);
        } else if character == '-' {
            self.0 |= 1 << HYPHEN_BIT_INDEX;
        }
    }
}

impl Extend<char> for CharBag {
    fn extend<T: IntoIterator<Item = char>>(&mut self, characters: T) {
        for character in characters {
            self.insert(character);
        }
    }
}

impl FromIterator<char> for CharBag {
    fn from_iter<T: IntoIterator<Item = char>>(characters: T) -> Self {
        let mut result = Self::default();
        result.extend(characters);
        result
    }
}

impl From<&str> for CharBag {
    fn from(string: &str) -> Self {
        string.chars().collect()
    }
}

impl From<&[char]> for CharBag {
    fn from(chars: &[char]) -> Self {
        chars.iter().copied().collect()
    }
}
