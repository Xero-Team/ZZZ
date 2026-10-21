use std::{any::Any, sync::Arc};

use any_vec::AnyVec;
use gpui::{
    AnyView, AnyWeakEntity, App, Context, Entity, EventEmitter, Subscription, Task, WeakEntity,
    Window,
};
use project::search::SearchQuery;
use settings::SeedQuerySetting;

use crate::{
    ItemHandle,
    item::{Item, WeakItemHandle},
};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchToken(u64);

impl SearchToken {
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    pub fn value(&self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone)]
pub enum SearchEvent {
    MatchesInvalidated,
    ActiveMatchChanged,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Direction {
    Prev,
    #[default]
    Next,
}

impl Direction {
    pub fn opposite(&self) -> Self {
        match self {
            Direction::Prev => Direction::Next,
            Direction::Next => Direction::Prev,
        }
    }
}

/// Wraps `current_index` into `0..matches_len` before stepping by `count`.
///
/// The active index can outlive the match list it was computed from while a new
/// search is replacing the results. Clamping first keeps a stale index from
/// escaping the slice that the caller then indexes.
fn wrapped_match_index(
    matches_len: usize,
    current_index: usize,
    direction: Direction,
    count: usize,
) -> usize {
    debug_assert!(
        matches_len > 0,
        "match index math requires at least one match"
    );

    let current_index = current_index.min(matches_len - 1);
    let count = count % matches_len;
    match direction {
        Direction::Prev => current_index
            .checked_sub(count)
            .unwrap_or_else(|| matches_len - (count - current_index)),
        Direction::Next => (current_index + count) % matches_len,
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SearchOptions {
    pub case: bool,
    pub word: bool,
    pub regex: bool,
    /// Specifies whether the  supports search & replace.
    pub replacement: bool,
    pub selection: bool,
    pub select_all: bool,
    pub find_in_results: bool,
}

// Whether to always select the current selection (even if empty)
// or to use the default (restoring the previous search ranges if some,
// otherwise using the whole file).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum FilteredSearchRange {
    Selection,
    #[default]
    Default,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct SelectSearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
}

pub trait SearchableItem: Item + EventEmitter<SearchEvent> {
    type Match: Any + Sync + Send + Clone;

    fn supported_options(&self) -> SearchOptions {
        SearchOptions {
            case: true,
            word: true,
            regex: true,
            replacement: true,
            selection: true,
            select_all: true,
            find_in_results: false,
        }
    }

    fn search_bar_visibility_changed(
        &mut self,
        _visible: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    fn has_filtered_search_ranges(&mut self) -> bool {
        self.supported_options().selection
    }

    fn toggle_filtered_search_ranges(
        &mut self,
        _enabled: Option<FilteredSearchRange>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    fn get_matches(&self, _window: &mut Window, _: &mut App) -> (Vec<Self::Match>, SearchToken) {
        (Vec::new(), SearchToken::default())
    }
    fn clear_matches(&mut self, window: &mut Window, cx: &mut Context<Self>);
    fn update_matches(
        &mut self,
        matches: &[Self::Match],
        active_match_index: Option<usize>,
        token: SearchToken,
        window: &mut Window,
        cx: &mut Context<Self>,
    );
    fn query_suggestion(
        &mut self,
        seed_query_override: Option<SeedQuerySetting>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> String;
    fn activate_match(
        &mut self,
        index: usize,
        matches: &[Self::Match],
        token: SearchToken,
        window: &mut Window,
        cx: &mut Context<Self>,
    );
    fn select_matches(
        &mut self,
        matches: &[Self::Match],
        token: SearchToken,
        window: &mut Window,
        cx: &mut Context<Self>,
    );
    fn replace(
        &mut self,
        _: &Self::Match,
        _: &SearchQuery,
        _token: SearchToken,
        _window: &mut Window,
        _: &mut Context<Self>,
    );
    fn replace_all(
        &mut self,
        matches: &mut dyn Iterator<Item = &Self::Match>,
        query: &SearchQuery,
        token: SearchToken,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for item in matches {
            self.replace(item, query, token, window, cx);
        }
    }
    fn match_index_for_direction(
        &mut self,
        matches: &[Self::Match],
        current_index: usize,
        direction: Direction,
        count: usize,
        _token: SearchToken,
        _window: &mut Window,
        _: &mut Context<Self>,
    ) -> usize {
        if matches.is_empty() {
            return current_index;
        }

        wrapped_match_index(matches.len(), current_index, direction, count)
    }
    fn find_matches(
        &mut self,
        query: Arc<SearchQuery>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Vec<Self::Match>>;

    fn find_matches_with_token(
        &mut self,
        query: Arc<SearchQuery>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<(Vec<Self::Match>, SearchToken)> {
        let matches = self.find_matches(query, window, cx);
        cx.spawn(async move |_, _| (matches.await, SearchToken::default()))
    }

    fn active_match_index(
        &mut self,
        direction: Direction,
        matches: &[Self::Match],
        token: SearchToken,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize>;
    fn set_select_search_options(&mut self, _: Option<SelectSearchOptions>, _: &mut Context<Self>) {
    }
}

pub trait SearchableItemHandle: ItemHandle {
    fn downgrade(&self) -> Box<dyn WeakSearchableItemHandle>;
    fn boxed_clone(&self) -> Box<dyn SearchableItemHandle>;
    fn supported_options(&self, cx: &App) -> SearchOptions;
    fn subscribe_to_search_events(
        &self,
        window: &mut Window,
        cx: &mut App,
        handler: Box<dyn Fn(&SearchEvent, &mut Window, &mut App) + Send>,
    ) -> Subscription;
    fn clear_matches(&self, window: &mut Window, cx: &mut App);
    fn update_matches(
        &self,
        matches: &AnyVec<dyn Send>,
        active_match_index: Option<usize>,
        token: SearchToken,
        window: &mut Window,
        cx: &mut App,
    );
    fn query_suggestion(
        &self,
        seed_query_override: Option<SeedQuerySetting>,
        window: &mut Window,
        cx: &mut App,
    ) -> String;
    fn activate_match(
        &self,
        index: usize,
        matches: &AnyVec<dyn Send>,
        token: SearchToken,
        window: &mut Window,
        cx: &mut App,
    );
    fn select_matches(
        &self,
        matches: &AnyVec<dyn Send>,
        token: SearchToken,
        window: &mut Window,
        cx: &mut App,
    );
    fn replace(
        &self,
        _: any_vec::element::ElementRef<'_, dyn Send>,
        _: &SearchQuery,
        token: SearchToken,
        _window: &mut Window,
        _: &mut App,
    );
    fn replace_all(
        &self,
        matches: &mut dyn Iterator<Item = any_vec::element::ElementRef<'_, dyn Send>>,
        query: &SearchQuery,
        token: SearchToken,
        window: &mut Window,
        cx: &mut App,
    );
    fn match_index_for_direction(
        &self,
        matches: &AnyVec<dyn Send>,
        current_index: usize,
        direction: Direction,
        count: usize,
        token: SearchToken,
        window: &mut Window,
        cx: &mut App,
    ) -> usize;
    fn find_matches(
        &self,
        query: Arc<SearchQuery>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<AnyVec<dyn Send>>;
    fn find_matches_with_token(
        &self,
        query: Arc<SearchQuery>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<(AnyVec<dyn Send>, SearchToken)>;
    fn active_match_index(
        &self,
        direction: Direction,
        matches: &AnyVec<dyn Send>,
        token: SearchToken,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<usize>;
    fn search_bar_visibility_changed(&self, visible: bool, window: &mut Window, cx: &mut App);

    fn toggle_filtered_search_ranges(
        &mut self,
        enabled: Option<FilteredSearchRange>,
        window: &mut Window,
        cx: &mut App,
    );

    fn set_select_search_options(&self, search_options: Option<SelectSearchOptions>, cx: &mut App);
}

impl<T: SearchableItem> SearchableItemHandle for Entity<T> {
    fn downgrade(&self) -> Box<dyn WeakSearchableItemHandle> {
        Box::new(self.downgrade())
    }

    fn boxed_clone(&self) -> Box<dyn SearchableItemHandle> {
        Box::new(self.clone())
    }

    fn supported_options(&self, cx: &App) -> SearchOptions {
        self.read(cx).supported_options()
    }

    fn subscribe_to_search_events(
        &self,
        window: &mut Window,
        cx: &mut App,
        handler: Box<dyn Fn(&SearchEvent, &mut Window, &mut App) + Send>,
    ) -> Subscription {
        window.subscribe(self, cx, move |_, event: &SearchEvent, window, cx| {
            handler(event, window, cx)
        })
    }

    fn clear_matches(&self, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| this.clear_matches(window, cx));
    }
    fn update_matches(
        &self,
        matches: &AnyVec<dyn Send>,
        active_match_index: Option<usize>,
        token: SearchToken,
        window: &mut Window,
        cx: &mut App,
    ) {
        let matches = matches.downcast_ref().expect("downcast should succeed");
        self.update(cx, |this, cx| {
            this.update_matches(matches.as_slice(), active_match_index, token, window, cx)
        });
    }
    fn query_suggestion(
        &self,
        seed_query_override: Option<SeedQuerySetting>,
        window: &mut Window,
        cx: &mut App,
    ) -> String {
        self.update(cx, |this, cx| {
            this.query_suggestion(seed_query_override, window, cx)
        })
    }
    fn activate_match(
        &self,
        index: usize,
        matches: &AnyVec<dyn Send>,
        token: SearchToken,
        window: &mut Window,
        cx: &mut App,
    ) {
        let matches = matches.downcast_ref().expect("downcast should succeed");
        self.update(cx, |this, cx| {
            this.activate_match(index, matches.as_slice(), token, window, cx)
        });
    }

    fn select_matches(
        &self,
        matches: &AnyVec<dyn Send>,
        token: SearchToken,
        window: &mut Window,
        cx: &mut App,
    ) {
        let matches = matches.downcast_ref().expect("downcast should succeed");
        self.update(cx, |this, cx| {
            this.select_matches(matches.as_slice(), token, window, cx)
        });
    }

    fn match_index_for_direction(
        &self,
        matches: &AnyVec<dyn Send>,
        current_index: usize,
        direction: Direction,
        count: usize,
        token: SearchToken,
        window: &mut Window,
        cx: &mut App,
    ) -> usize {
        let matches = matches.downcast_ref().expect("downcast should succeed");
        self.update(cx, |this, cx| {
            this.match_index_for_direction(
                matches.as_slice(),
                current_index,
                direction,
                count,
                token,
                window,
                cx,
            )
        })
    }
    fn find_matches(
        &self,
        query: Arc<SearchQuery>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<AnyVec<dyn Send>> {
        let matches = self.update(cx, |this, cx| this.find_matches(query, window, cx));
        window.spawn(cx, async |_| {
            let matches = matches.await;
            let mut any_matches = AnyVec::with_capacity::<T::Match>(matches.len());
            {
                let mut any_matches = any_matches
                    .downcast_mut::<T::Match>()
                    .expect("downcast should succeed");
                for mat in matches {
                    any_matches.push(mat);
                }
            }
            any_matches
        })
    }
    fn find_matches_with_token(
        &self,
        query: Arc<SearchQuery>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<(AnyVec<dyn Send>, SearchToken)> {
        let matches_with_token = self.update(cx, |this, cx| {
            this.find_matches_with_token(query, window, cx)
        });
        window.spawn(cx, async |_| {
            let (matches, token) = matches_with_token.await;
            let mut any_matches = AnyVec::with_capacity::<T::Match>(matches.len());
            {
                let mut any_matches = any_matches
                    .downcast_mut::<T::Match>()
                    .expect("downcast should succeed");
                for mat in matches {
                    any_matches.push(mat);
                }
            }
            (any_matches, token)
        })
    }
    fn active_match_index(
        &self,
        direction: Direction,
        matches: &AnyVec<dyn Send>,
        token: SearchToken,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<usize> {
        let matches = matches.downcast_ref()?;
        self.update(cx, |this, cx| {
            this.active_match_index(direction, matches.as_slice(), token, window, cx)
        })
    }

    fn replace(
        &self,
        mat: any_vec::element::ElementRef<'_, dyn Send>,
        query: &SearchQuery,
        token: SearchToken,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mat = mat.downcast_ref().expect("downcast should succeed");
        self.update(cx, |this, cx| this.replace(mat, query, token, window, cx))
    }

    fn replace_all(
        &self,
        matches: &mut dyn Iterator<Item = any_vec::element::ElementRef<'_, dyn Send>>,
        query: &SearchQuery,
        token: SearchToken,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.update(cx, |this, cx| {
            this.replace_all(
                &mut matches.map(|m| m.downcast_ref().expect("downcast should succeed")),
                query,
                token,
                window,
                cx,
            );
        })
    }

    fn search_bar_visibility_changed(&self, visible: bool, window: &mut Window, cx: &mut App) {
        self.update(cx, |this, cx| {
            this.search_bar_visibility_changed(visible, window, cx)
        });
    }

    fn toggle_filtered_search_ranges(
        &mut self,
        enabled: Option<FilteredSearchRange>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.update(cx, |this, cx| {
            this.toggle_filtered_search_ranges(enabled, window, cx)
        });
    }
    fn set_select_search_options(&self, search_options: Option<SelectSearchOptions>, cx: &mut App) {
        self.update(cx, |this, cx| {
            this.set_select_search_options(search_options, cx)
        });
    }
}

impl From<Box<dyn SearchableItemHandle>> for AnyView {
    fn from(this: Box<dyn SearchableItemHandle>) -> Self {
        this.to_any_view()
    }
}

impl From<&Box<dyn SearchableItemHandle>> for AnyView {
    fn from(this: &Box<dyn SearchableItemHandle>) -> Self {
        this.to_any_view()
    }
}

impl PartialEq for Box<dyn SearchableItemHandle> {
    fn eq(&self, other: &Self) -> bool {
        self.item_id() == other.item_id()
    }
}

impl Eq for Box<dyn SearchableItemHandle> {}

pub trait WeakSearchableItemHandle: WeakItemHandle {
    fn upgrade(&self, cx: &App) -> Option<Box<dyn SearchableItemHandle>>;

    fn into_any(self) -> AnyWeakEntity;
}

impl<T: SearchableItem> WeakSearchableItemHandle for WeakEntity<T> {
    fn upgrade(&self, _cx: &App) -> Option<Box<dyn SearchableItemHandle>> {
        Some(Box::new(self.upgrade()?))
    }

    fn into_any(self) -> AnyWeakEntity {
        self.into()
    }
}

impl PartialEq for Box<dyn WeakSearchableItemHandle> {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Eq for Box<dyn WeakSearchableItemHandle> {}

impl std::hash::Hash for Box<dyn WeakSearchableItemHandle> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state)
    }
}

#[cfg(test)]
mod tests {
    use super::{Direction, wrapped_match_index};

    #[test]
    fn wrapped_match_index_handles_stale_and_large_counts() {
        assert_eq!(wrapped_match_index(3, 0, Direction::Prev, 1), 2);
        assert_eq!(wrapped_match_index(3, 2, Direction::Next, 1), 0);

        // A stale active index from a longer match list must stay in bounds.
        assert_eq!(wrapped_match_index(3, 9, Direction::Prev, 1), 1);
        assert_eq!(wrapped_match_index(3, 9, Direction::Next, 1), 0);

        // Large counts must not overflow the addition.
        assert_eq!(wrapped_match_index(4, 1, Direction::Next, usize::MAX), 0);
        assert_eq!(wrapped_match_index(4, 1, Direction::Prev, usize::MAX), 2);
    }
}
