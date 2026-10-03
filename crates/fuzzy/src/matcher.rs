use std::{
    borrow::Borrow,
    sync::atomic::{self, AtomicBool},
};

use crate::{CharBag, char_bag::simple_lowercase};

const BASE_DISTANCE_PENALTY: f64 = 0.6;
const ADDITIONAL_DISTANCE_PENALTY: f64 = 0.05;
const MIN_DISTANCE_PENALTY: f64 = 0.2;

pub struct Matcher<'a> {
    query: &'a [char],
    lowercase_query: &'a [char],
    query_char_bag: CharBag,
    smart_case: bool,
    penalize_length: bool,
    min_score: f64,
    match_positions: Vec<usize>,
    last_positions: Vec<usize>,
    score_matrix: Vec<Option<f64>>,
    best_position_matrix: Vec<usize>,
}

pub trait MatchCandidate {
    fn has_chars(&self, bag: CharBag) -> bool;
    fn candidate_chars(&self) -> impl Iterator<Item = char>;
}

impl<'a> Matcher<'a> {
    pub fn new(
        query: &'a [char],
        lowercase_query: &'a [char],
        query_char_bag: CharBag,
        smart_case: bool,
        penalize_length: bool,
    ) -> Self {
        Self {
            query,
            lowercase_query,
            query_char_bag,
            min_score: 0.0,
            last_positions: vec![0; lowercase_query.len()],
            match_positions: vec![0; query.len()],
            score_matrix: Vec::new(),
            best_position_matrix: Vec::new(),
            smart_case,
            penalize_length,
        }
    }

    /// Filter and score fuzzy match candidates. Results are returned unsorted, in the same order as
    /// the input candidates.
    pub(crate) fn match_candidates<C, R, F, T>(
        &mut self,
        prefix: &[char],
        lowercase_prefix: &[char],
        candidates: impl Iterator<Item = T>,
        results: &mut Vec<R>,
        cancel_flag: &AtomicBool,
        build_match: F,
    ) where
        C: MatchCandidate,
        T: Borrow<C>,
        F: Fn(&C, f64, &[usize]) -> R,
    {
        let mut candidate_chars = Vec::new();
        let mut lowercase_candidate_chars = Vec::new();

        for candidate in candidates {
            if !candidate.borrow().has_chars(self.query_char_bag) {
                continue;
            }

            if cancel_flag.load(atomic::Ordering::Acquire) {
                break;
            }

            candidate_chars.clear();
            lowercase_candidate_chars.clear();
            for character in candidate.borrow().candidate_chars() {
                candidate_chars.push(character);
                lowercase_candidate_chars.push(simple_lowercase(character));
            }

            if !self.find_last_positions(lowercase_prefix, &lowercase_candidate_chars) {
                continue;
            }

            let matrix_len =
                self.query.len() * (lowercase_prefix.len() + lowercase_candidate_chars.len());
            self.score_matrix.clear();
            self.score_matrix.resize(matrix_len, None);
            self.best_position_matrix.clear();
            self.best_position_matrix.resize(matrix_len, 0);

            let score = self.score_match(
                &candidate_chars,
                &lowercase_candidate_chars,
                prefix,
                lowercase_prefix,
            );

            if score > 0.0 {
                results.push(build_match(
                    candidate.borrow(),
                    score,
                    &self.match_positions,
                ));
            }
        }
    }

    fn find_last_positions(
        &mut self,
        lowercase_prefix: &[char],
        lowercase_candidate: &[char],
    ) -> bool {
        let mut lowercase_prefix = lowercase_prefix.iter();
        let mut lowercase_candidate = lowercase_candidate.iter();
        for (query_index, query_character) in self.lowercase_query.iter().enumerate().rev() {
            if let Some(candidate_index) =
                lowercase_candidate.rposition(|character| character == query_character)
            {
                self.last_positions[query_index] = candidate_index + lowercase_prefix.len();
            } else if let Some(prefix_index) =
                lowercase_prefix.rposition(|character| character == query_character)
            {
                self.last_positions[query_index] = prefix_index;
            } else {
                return false;
            }
        }
        true
    }

    fn score_match(
        &mut self,
        path: &[char],
        path_lowercased: &[char],
        prefix: &[char],
        lowercase_prefix: &[char],
    ) -> f64 {
        let score = self.recursive_score_match(
            path,
            path_lowercased,
            prefix,
            lowercase_prefix,
            0,
            0,
            self.query.len() as f64,
        ) * self.query.len() as f64;

        if score <= 0.0 {
            return 0.0;
        }
        let path_len = prefix.len() + path.len();
        let mut current_start = 0;
        let mut byte_index = 0;
        let mut character_index = 0;
        for query_index in 0..self.query.len() {
            let matched_character_index =
                self.best_position_matrix[query_index * path_len + current_start];
            while character_index < matched_character_index {
                let character = prefix
                    .get(character_index)
                    .or_else(|| path.get(character_index - prefix.len()))
                    .expect("entry should be present");
                byte_index += character.len_utf8();
                character_index += 1;
            }

            self.match_positions[query_index] = byte_index;

            let matched_character = prefix
                .get(matched_character_index)
                .or_else(|| path.get(matched_character_index - prefix.len()))
                .expect("entry should be present");
            byte_index += matched_character.len_utf8();

            current_start = matched_character_index + 1;
            character_index = matched_character_index + 1;
        }

        score
    }

    fn recursive_score_match(
        &mut self,
        path: &[char],
        path_lowercased: &[char],
        prefix: &[char],
        lowercase_prefix: &[char],
        query_index: usize,
        path_index: usize,
        current_score: f64,
    ) -> f64 {
        if query_index == self.query.len() {
            return 1.0;
        }

        let limit = self.last_positions[query_index];
        let max_valid_index = (prefix.len() + path_lowercased.len()).saturating_sub(1);
        let safe_limit = limit.min(max_valid_index);

        if path_index > safe_limit {
            return 0.0;
        }

        let path_len = prefix.len() + path.len();
        if let Some(memoized) = self.score_matrix[query_index * path_len + path_index] {
            return memoized;
        }

        let mut score = 0.0;
        let mut best_position = 0;

        let query_character = self.lowercase_query[query_index];

        let mut last_slash = 0;

        for candidate_index in path_index..=safe_limit {
            let path_character = if candidate_index < prefix.len() {
                lowercase_prefix[candidate_index]
            } else {
                let relative_path_index = candidate_index - prefix.len();
                match path_lowercased.get(relative_path_index) {
                    Some(&character) => character,
                    None => continue,
                }
            };
            let is_path_separator = path_character == '/';

            if query_index == 0 && is_path_separator {
                last_slash = candidate_index;
            }
            let need_to_score =
                query_character == path_character || (is_path_separator && query_character == '_');
            if need_to_score {
                let current_character = match prefix.get(candidate_index) {
                    Some(&current_character) => current_character,
                    None => path[candidate_index - prefix.len()],
                };

                let mut char_score = 1.0;
                if candidate_index > path_index {
                    let previous_character = match prefix.get(candidate_index - 1) {
                        Some(&previous_character) => previous_character,
                        None => path[candidate_index - 1 - prefix.len()],
                    };

                    if previous_character == '/' {
                        char_score = 0.9;
                    } else if (previous_character == '-'
                        || previous_character == '_'
                        || previous_character == ' '
                        || previous_character.is_numeric())
                        || (previous_character.is_lowercase() && current_character.is_uppercase())
                    {
                        char_score = 0.8;
                    } else if previous_character == '.' {
                        char_score = 0.7;
                    } else if query_index == 0 {
                        char_score = BASE_DISTANCE_PENALTY;
                    } else {
                        char_score = MIN_DISTANCE_PENALTY.max(
                            BASE_DISTANCE_PENALTY
                                - (candidate_index - path_index - 1) as f64
                                    * ADDITIONAL_DISTANCE_PENALTY,
                        );
                    }
                }

                // Apply a severe penalty if the case doesn't match.
                // This will make the exact matches have higher score than the case-insensitive and the
                // path insensitive matches.
                if (self.smart_case || current_character == '/')
                    && self.query[query_index] != current_character
                {
                    char_score *= 0.001;
                }

                let mut multiplier = char_score;

                // Scale the score based on how deep within the path we found the match.
                if self.penalize_length && query_index == 0 {
                    multiplier /= ((prefix.len() + path.len()) - last_slash) as f64;
                }

                let mut next_score = 1.0;
                if self.min_score > 0.0 {
                    next_score = current_score * multiplier;
                    // Scores only decrease. If we can't pass the previous best, bail
                    if next_score < self.min_score {
                        // Ensure that score is non-zero so we use it in the memo table.
                        if score == 0.0 {
                            score = 1e-18;
                        }
                        continue;
                    }
                }

                let new_score = self.recursive_score_match(
                    path,
                    path_lowercased,
                    prefix,
                    lowercase_prefix,
                    query_index + 1,
                    candidate_index + 1,
                    next_score,
                ) * multiplier;

                if new_score > score {
                    score = new_score;
                    best_position = candidate_index;
                    // Optimization: can't score better than 1.
                    if new_score == 1.0 {
                        break;
                    }
                }
            }
        }

        if best_position != 0 {
            self.best_position_matrix[query_index * path_len + path_index] = best_position;
        }

        self.score_matrix[query_index * path_len + path_index] = Some(score);
        score
    }
}

#[cfg(test)]
mod tests {
    use util::rel_path::{RelPath, rel_path};

    use crate::{PathMatch, PathMatchCandidate};

    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_get_last_positions() {
        let mut query: &[char] = &['d', 'c'];
        let mut matcher = Matcher::new(query, query, query.into(), false, true);
        let result = matcher.find_last_positions(&['a', 'b', 'c'], &['b', 'd', 'e', 'f']);
        assert!(!result);

        query = &['c', 'd'];
        let mut matcher = Matcher::new(query, query, query.into(), false, true);
        let result = matcher.find_last_positions(&['a', 'b', 'c'], &['b', 'd', 'e', 'f']);
        assert!(result);
        assert_eq!(matcher.last_positions, vec![2, 4]);

        query = &['z', '/', 'z', 'f'];
        let mut matcher = Matcher::new(query, query, query.into(), false, true);
        let result = matcher.find_last_positions(&['z', 'e', 'd', '/'], &['z', 'e', 'd', '/', 'f']);
        assert!(result);
        assert_eq!(matcher.last_positions, vec![0, 3, 4, 8]);
    }

    #[test]
    fn test_match_path_entries() {
        let paths = vec![
            "",
            "a",
            "ab",
            "abC",
            "abcd",
            "alphabravocharlie",
            "AlphaBravoCharlie",
            "thisisatestdir",
            "ThisIsATestDir",
            "this/is/a/test/dir",
            "test/tiatd",
        ];

        assert_eq!(
            match_single_path_query("abc", false, &paths),
            vec![
                ("abC", vec![0, 1, 2]),
                ("abcd", vec![0, 1, 2]),
                ("AlphaBravoCharlie", vec![0, 5, 10]),
                ("alphabravocharlie", vec![4, 5, 10]),
            ]
        );
        assert_eq!(
            match_single_path_query("t/i/a/t/d", false, &paths),
            vec![("this/is/a/test/dir", vec![0, 4, 5, 7, 8, 9, 10, 14, 15]),]
        );

        assert_eq!(
            match_single_path_query("tiatd", false, &paths),
            vec![
                ("test/tiatd", vec![5, 6, 7, 8, 9]),
                ("ThisIsATestDir", vec![0, 4, 6, 7, 11]),
                ("this/is/a/test/dir", vec![0, 5, 8, 10, 15]),
                ("thisisatestdir", vec![0, 2, 6, 7, 11]),
            ]
        );
    }

    #[test]
    fn test_lowercase_longer_than_uppercase() {
        // This character has more chars in lower-case than in upper-case.
        let paths = vec!["\u{0130}"];
        let query = "\u{0130}";
        assert_eq!(
            match_single_path_query(query, false, &paths),
            vec![("\u{0130}", vec![0])]
        );

        // Path is the lower-case version of the query
        let paths = vec!["i\u{307}"];
        let query = "\u{0130}";
        assert_eq!(
            match_single_path_query(query, false, &paths),
            vec![("i\u{307}", vec![0])]
        );
    }

    #[test]
    fn test_match_multibyte_path_entries() {
        let paths = vec![
            "aαbβ/cγdδ",
            "αβγδ/bcde",
            "c1️⃣2️⃣3️⃣/d4️⃣5️⃣6️⃣/e7️⃣8️⃣9️⃣/f",
            "d/🆒/h",
        ];
        assert_eq!("1️⃣".len(), 7);
        assert_eq!(
            match_single_path_query("bcd", false, &paths),
            vec![
                ("αβγδ/bcde", vec![9, 10, 11]),
                ("aαbβ/cγdδ", vec![3, 7, 10]),
            ]
        );
        assert_eq!(
            match_single_path_query("cde", false, &paths),
            vec![
                ("αβγδ/bcde", vec![10, 11, 12]),
                ("c1️⃣2️⃣3️⃣/d4️⃣5️⃣6️⃣/e7️⃣8️⃣9️⃣/f", vec![0, 23, 46]),
            ]
        );
    }

    #[test]
    fn match_unicode_path_entries() {
        let mixed_unicode_paths = vec![
            "İolu/oluş",
            "İstanbul/code",
            "Athens/Şanlıurfa",
            "Çanakkale/scripts",
            "paris/Düzce_İl",
            "Berlin_Önemli_Ğündem",
            "KİTAPLIK/london/dosya",
            "tokyo/kyoto/fuji",
            "new_york/san_francisco",
        ];

        assert_eq!(
            match_single_path_query("İo/oluş", false, &mixed_unicode_paths),
            vec![("İolu/oluş", vec![0, 2, 5, 6, 7, 8, 9])]
        );

        assert_eq!(
            match_single_path_query("İst/code", false, &mixed_unicode_paths),
            vec![("İstanbul/code", vec![0, 2, 3, 9, 10, 11, 12, 13])]
        );

        assert_eq!(
            match_single_path_query("athens/şa", false, &mixed_unicode_paths),
            vec![("Athens/Şanlıurfa", vec![0, 1, 2, 3, 4, 5, 6, 7, 9])]
        );

        assert_eq!(
            match_single_path_query("BerlinÖĞ", false, &mixed_unicode_paths),
            vec![("Berlin_Önemli_Ğündem", vec![0, 1, 2, 3, 4, 5, 7, 15])]
        );

        assert_eq!(
            match_single_path_query("tokyo/fuji", false, &mixed_unicode_paths),
            vec![("tokyo/kyoto/fuji", vec![0, 1, 2, 3, 4, 5, 12, 13, 14, 15])]
        );

        let mixed_script_paths = vec![
            "résumé_Москва",
            "naïve_київ_implementation",
            "café_北京_app",
            "東京_über_driver",
            "déjà_vu_cairo",
            "seoul_piñata_game",
            "voilà_istanbul_result",
        ];

        assert_eq!(
            match_single_path_query("résmé", false, &mixed_script_paths),
            vec![("résumé_Москва", vec![0, 1, 3, 5, 6])]
        );

        assert_eq!(
            match_single_path_query("café北京", false, &mixed_script_paths),
            vec![("café_北京_app", vec![0, 1, 2, 3, 6, 9])]
        );

        assert_eq!(
            match_single_path_query("ista", false, &mixed_script_paths),
            vec![("voilà_istanbul_result", vec![7, 8, 9, 10])]
        );

        let complex_paths = vec![
            "document_📚_library",
            "project_👨‍👩‍👧‍👦_family",
            "flags_🇯🇵🇺🇸🇪🇺_world",
            "code_😀😃😄😁_happy",
            "photo_👩‍👩‍👧‍👦_album",
        ];

        assert_eq!(
            match_single_path_query("doc📚lib", false, &complex_paths),
            vec![("document_📚_library", vec![0, 1, 2, 9, 14, 15, 16])]
        );

        assert_eq!(
            match_single_path_query("codehappy", false, &complex_paths),
            vec![("code_😀😃😄😁_happy", vec![0, 1, 2, 3, 22, 23, 24, 25, 26])]
        );
    }

    #[test]
    fn test_positions_are_valid_char_boundaries_with_expanding_lowercase() {
        // İ (U+0130) lowercases to "i\u{307}" (2 chars) under full case folding.
        // With simple case mapping (used by this matcher), İ → 'i' (1 char),
        // so positions remain valid byte boundaries.
        let paths = vec!["İstanbul/code.rs", "aİbİc/dİeİf.txt", "src/İmport/İndex.ts"];

        for query in &["code", "İst", "dİe", "İndex", "İmport", "abcdef"] {
            let results = match_single_path_query(query, false, &paths);
            for (path, positions) in &results {
                for &pos in positions {
                    assert!(
                        path.is_char_boundary(pos),
                        "Position {pos} is not a valid char boundary in path {path:?} \
                         (query: {query:?}, all positions: {positions:?})"
                    );
                }
            }
        }
    }

    #[test]
    fn test_positions_valid_with_various_multibyte_chars() {
        // German ß uppercases to SS but lowercases to itself — no expansion.
        // Armenian ligatures and other characters that could expand under full
        // case folding should still produce valid byte boundaries.
        let paths = vec![
            "straße/config.rs",
            "Straße/München/file.txt",
            "ﬁle/path.rs",     // ﬁ (U+FB01, fi ligature)
            "ﬀoo/bar.txt",     // ﬀ (U+FB00, ff ligature)
            "aÇbŞc/dÖeÜf.txt", // Turkish chars that don't expand
        ];

        for query in &["config", "Mün", "file", "bar", "abcdef", "straße", "ÇŞ"] {
            let results = match_single_path_query(query, false, &paths);
            for (path, positions) in &results {
                for &pos in positions {
                    assert!(
                        path.is_char_boundary(pos),
                        "Position {pos} is not a valid char boundary in path {path:?} \
                         (query: {query:?}, all positions: {positions:?})"
                    );
                }
            }
        }
    }

    fn match_single_path_query<'a>(
        query: &str,
        smart_case: bool,
        paths: &[&'a str],
    ) -> Vec<(&'a str, Vec<usize>)> {
        let lowercase_query = query.chars().map(simple_lowercase).collect::<Vec<_>>();
        let query = query.chars().collect::<Vec<_>>();
        let query_chars = CharBag::from(lowercase_query.as_slice());

        let path_arcs: Vec<Arc<RelPath>> = paths
            .iter()
            .map(|path| Arc::from(rel_path(path)))
            .collect::<Vec<_>>();
        let mut path_entries = Vec::new();
        for (i, path) in paths.iter().enumerate() {
            let lowercase_path: Vec<char> = path.chars().map(simple_lowercase).collect();
            let char_bag = CharBag::from(lowercase_path.as_slice());
            path_entries.push(PathMatchCandidate {
                is_dir: false,
                char_bag,
                path: &path_arcs[i],
            });
        }

        let mut matcher = Matcher::new(&query, &lowercase_query, query_chars, smart_case, true);

        let cancel_flag = AtomicBool::new(false);
        let mut results = Vec::new();

        matcher.match_candidates(
            &[],
            &[],
            path_entries.into_iter(),
            &mut results,
            &cancel_flag,
            |candidate, score, positions| PathMatch {
                score,
                worktree_id: 0,
                positions: positions.to_vec(),
                path: candidate.path.into(),
                path_prefix: RelPath::empty_arc(),
                distance_to_relative_ancestor: usize::MAX,
                is_dir: false,
            },
        );
        results.sort_by(|left, right| right.cmp(left));

        results
            .into_iter()
            .map(|result| {
                (
                    paths
                        .iter()
                        .copied()
                        .find(|path| result.path.as_ref() == rel_path(path))
                        .unwrap(),
                    result.positions,
                )
            })
            .collect()
    }

    /// Test for https://github.com/zed-industries/zed/issues/44324
    #[test]
    fn test_recursive_score_match_index_out_of_bounds() {
        let paths = vec!["İ/İ/İ/İ"];
        let query = "İ/İ";

        // This panicked with "index out of bounds: the len is 21 but the index is 22"
        match_single_path_query(query, false, &paths);
    }
}
