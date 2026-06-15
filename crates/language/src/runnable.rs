use std::{cmp::Reverse, iter, ops::Range, sync::Arc};

use collections::HashMap;
use smallvec::SmallVec;
use text::BufferId;
use tree_sitter::QueryCapture;
use util::RangeExt;

use crate::{BufferSnapshot, Language, Runnable, RunnableCapture, RunnableConfig, RunnableTag};

pub struct RunnableRange {
    pub buffer_id: BufferId,
    pub run_range: Range<usize>,
    pub full_range: Range<usize>,
    pub runnable: Runnable,
    pub extra_captures: HashMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct RunnableMatchCapture {
    range: Range<usize>,
    capture: ResolverCapture,
}

#[derive(Clone, Debug)]
enum ResolverCapture {
    Run,
    Named(String),
}

impl RunnableMatchCapture {
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    pub fn is_run(&self) -> bool {
        matches!(self.capture, ResolverCapture::Run)
    }

    pub fn name(&self) -> Option<&str> {
        match &self.capture {
            ResolverCapture::Named(name) => Some(name.as_str()),
            ResolverCapture::Run => None,
        }
    }
}

pub struct ResolvedRunnable {
    pub run_range: Range<usize>,
    pub extra_captures: SmallVec<[(String, String); 2]>,
}

pub trait RunnableResolver: Send + Sync {
    fn resolve(
        &self,
        local_captures: &[RunnableMatchCapture],
        shared_captures: &[RunnableMatchCapture],
        buffer: &BufferSnapshot,
    ) -> Option<ResolvedRunnable>;
}

pub(crate) fn runnable_ranges(
    buffer: &BufferSnapshot,
    offset_range: Range<usize>,
) -> impl Iterator<Item = RunnableRange> + '_ {
    let mut syntax_matches = buffer.matches(offset_range.clone(), |grammar| {
        grammar.runnable_config.as_ref().map(|config| &config.query)
    });

    let runnable_configs = syntax_matches
        .grammars()
        .iter()
        .map(|grammar| grammar.runnable_config.as_ref())
        .collect::<Vec<_>>();

    iter::from_fn(move || -> Option<SmallVec<[RunnableRange; 1]>> {
        let mat = syntax_matches.peek()?;

        let ranges = match runnable_configs[mat.grammar_index] {
            Some(runnable_config) => {
                let is_grouped = runnable_config.supports_grouped_runnables
                    && mat.captures.iter().any(|capture| {
                        matches!(
                            runnable_config.extra_captures.get(capture.index as usize),
                            Some(RunnableCapture::RunItem)
                        )
                    });
                if is_grouped {
                    runnable_ranges_from_grouped_matches(
                        buffer,
                        mat.captures,
                        runnable_config,
                        mat.pattern_index,
                        mat.language.clone(),
                        offset_range.clone(),
                    )
                } else {
                    runnable_range_from_captures(
                        buffer,
                        mat.captures,
                        runnable_config,
                        mat.pattern_index,
                        mat.language,
                    )
                    .into_iter()
                    .collect()
                }
            }
            None => SmallVec::new(),
        };

        syntax_matches.advance();
        Some(ranges)
    })
    .flatten()
}

type RunnableMatchCaptures = SmallVec<[RunnableMatchCapture; 4]>;

struct RunnableMatchGroup {
    range: Range<usize>,
    captures: RunnableMatchCaptures,
}

struct GroupedRunnableMatches {
    groups: SmallVec<[RunnableMatchGroup; 1]>,
    shared_captures: RunnableMatchCaptures,
}

fn runnable_tags_from_pattern(
    query: &tree_sitter::Query,
    pattern_index: usize,
) -> SmallVec<[RunnableTag; 1]> {
    query
        .property_settings(pattern_index)
        .iter()
        .filter_map(|property| {
            if *property.key == *"tag" {
                property
                    .value
                    .as_ref()
                    .map(|value| RunnableTag(value.to_string().into()))
            } else {
                None
            }
        })
        .collect()
}

fn range_overlaps_or_contains(range: &Range<usize>, offset_range: &Range<usize>) -> bool {
    if offset_range.is_empty() {
        range.contains(&offset_range.start)
    } else {
        range.overlaps(offset_range)
    }
}

fn group_runnable_matches(
    captures: &[QueryCapture<'_>],
    runnable_config: &RunnableConfig,
    offset_range: Range<usize>,
) -> GroupedRunnableMatches {
    let mut sorted: SmallVec<[&QueryCapture<'_>; 16]> = captures.iter().collect();
    sorted.sort_by_key(|capture| {
        let range = capture.node.byte_range();
        (range.start, Reverse(range.end))
    });

    let mut groups = SmallVec::new();
    let mut shared_captures = SmallVec::new();
    let mut current_group: Option<RunnableMatchGroup> = None;
    let mut current_in_offset = false;

    for capture in sorted {
        let range = capture.node.byte_range();
        let Some(kind) = runnable_config.extra_captures.get(capture.index as usize) else {
            continue;
        };

        let resolver_capture = match kind {
            RunnableCapture::RunItem => {
                if let Some(group) = current_group.take()
                    && current_in_offset
                {
                    groups.push(group);
                }
                current_in_offset = range_overlaps_or_contains(&range, &offset_range);
                current_group = Some(RunnableMatchGroup {
                    range,
                    captures: SmallVec::new(),
                });
                continue;
            }
            RunnableCapture::Run => ResolverCapture::Run,
            RunnableCapture::Named(name) => ResolverCapture::Named(name.to_string()),
        };

        let match_capture = RunnableMatchCapture {
            range: range.clone(),
            capture: resolver_capture,
        };

        match current_group.as_mut() {
            Some(group) if group.range.contains_inclusive(&range) => {
                if current_in_offset {
                    group.captures.push(match_capture);
                }
            }
            _ => {
                if let Some(group) = current_group.take()
                    && current_in_offset
                {
                    groups.push(group);
                }
                shared_captures.push(match_capture);
            }
        }
    }
    if let Some(group) = current_group.take()
        && current_in_offset
    {
        groups.push(group);
    }

    GroupedRunnableMatches {
        groups,
        shared_captures,
    }
}

fn runnable_ranges_from_grouped_matches(
    buffer: &BufferSnapshot,
    captures: &[QueryCapture<'_>],
    runnable_config: &RunnableConfig,
    pattern_index: usize,
    language: Arc<Language>,
    offset_range: Range<usize>,
) -> SmallVec<[RunnableRange; 1]> {
    let GroupedRunnableMatches {
        groups,
        shared_captures,
    } = group_runnable_matches(captures, runnable_config, offset_range);

    let shared_extras: SmallVec<[(String, String); 4]> = shared_captures
        .iter()
        .filter_map(|capture| {
            capture.name().map(|name| {
                (
                    name.to_owned(),
                    buffer.text_for_range(capture.range()).collect::<String>(),
                )
            })
        })
        .collect();

    let Some(resolver) = language
        .context_provider()
        .and_then(|provider| provider.runnable_resolver())
    else {
        return SmallVec::new();
    };

    let tags = runnable_tags_from_pattern(&runnable_config.query, pattern_index);
    let buffer_id = buffer.remote_id();
    let mut runnable_ranges = SmallVec::with_capacity(groups.len());

    for group in groups {
        let Some(ResolvedRunnable {
            run_range,
            extra_captures: local_extras,
        }) = resolver.resolve(&group.captures, &shared_captures, buffer)
        else {
            continue;
        };

        let extra_captures = shared_extras.iter().cloned().chain(local_extras).collect();

        runnable_ranges.push(RunnableRange {
            run_range,
            full_range: group.range,
            runnable: Runnable {
                tags: tags.clone(),
                language: language.clone(),
                buffer: buffer_id,
            },
            extra_captures,
            buffer_id,
        });
    }

    runnable_ranges
}

fn runnable_range_from_captures(
    buffer: &BufferSnapshot,
    captures: &[QueryCapture<'_>],
    runnable_config: &RunnableConfig,
    pattern_index: usize,
    language: Arc<Language>,
) -> Option<RunnableRange> {
    let mut run_range = None;
    let first_capture = captures.first()?;
    let full_range =
        captures
            .iter()
            .skip(1)
            .fold(first_capture.node.byte_range(), |mut acc, next| {
                let byte_range = next.node.byte_range();
                if acc.start > byte_range.start {
                    acc.start = byte_range.start;
                }
                if acc.end < byte_range.end {
                    acc.end = byte_range.end;
                }
                acc
            });

    let extra_captures: SmallVec<[_; 1]> =
        SmallVec::from_iter(captures.iter().filter_map(|capture| {
            runnable_config
                .extra_captures
                .get(capture.index as usize)
                .cloned()
                .and_then(|tag_name| match tag_name {
                    RunnableCapture::Named(name) => Some((capture.node.byte_range(), name)),
                    RunnableCapture::Run => {
                        let _ = run_range.insert(capture.node.byte_range());
                        None
                    }
                    RunnableCapture::RunItem => None,
                })
        }));
    let run_range = run_range?;
    let tags = runnable_tags_from_pattern(&runnable_config.query, pattern_index);
    let extra_captures = extra_captures
        .into_iter()
        .map(|(range, name)| {
            (
                name.to_string(),
                buffer.text_for_range(range).collect::<String>(),
            )
        })
        .collect();

    let buffer_id = buffer.remote_id();
    Some(RunnableRange {
        run_range,
        full_range,
        runnable: Runnable {
            tags,
            language,
            buffer: buffer_id,
        },
        extra_captures,
        buffer_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Buffer, ContextProvider, Language, LanguageConfig, LanguageMatcher, LanguageQueries,
    };
    use gpui::{AppContext as _, TestAppContext};
    use indoc::indoc;
    use std::{borrow::Cow, sync::Arc};

    struct TestContextProvider {
        resolver: Arc<dyn RunnableResolver>,
    }

    impl ContextProvider for TestContextProvider {
        fn runnable_resolver(&self) -> Option<Arc<dyn RunnableResolver>> {
            Some(self.resolver.clone())
        }
    }

    struct FirstRunResolver;

    impl RunnableResolver for FirstRunResolver {
        fn resolve(
            &self,
            local_captures: &[RunnableMatchCapture],
            _shared_captures: &[RunnableMatchCapture],
            _buffer: &BufferSnapshot,
        ) -> Option<ResolvedRunnable> {
            let run = local_captures.iter().find(|capture| capture.is_run())?;
            Some(ResolvedRunnable {
                run_range: run.range(),
                extra_captures: SmallVec::new(),
            })
        }
    }

    fn make_language(
        runnables_query: &'static str,
        resolver: Option<Arc<dyn RunnableResolver>>,
    ) -> Arc<Language> {
        let language = Language::new(
            LanguageConfig {
                name: "Rust".into(),
                matcher: LanguageMatcher {
                    path_suffixes: vec!["rs".to_string()],
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_queries(LanguageQueries {
            runnables: Some(Cow::Borrowed(runnables_query)),
            ..Default::default()
        })
        .expect("parse runnables query");
        let context_provider = resolver
            .map(|resolver| Arc::new(TestContextProvider { resolver }) as Arc<dyn ContextProvider>);
        Arc::new(language.with_context_provider(context_provider))
    }

    fn collect_runnables(
        cx: &mut TestAppContext,
        source: &str,
        runnables_query: &'static str,
        resolver: Option<Arc<dyn RunnableResolver>>,
        range: Range<usize>,
    ) -> Vec<RunnableRange> {
        let language = make_language(runnables_query, resolver);
        let buffer = cx.new(|cx| Buffer::local(source.to_string(), cx).with_language(language, cx));
        cx.executor().run_until_parked();
        buffer.update(cx, |buffer, _| {
            buffer.snapshot().runnable_ranges(range).collect()
        })
    }

    const GROUPED_QUERY: &str = indoc! {r#"
        (function_item
          name: (identifier) @_outer
          body: (block
            ((expression_statement
               (call_expression
                 function: (identifier) @run @_call)) @run_item)+))
    "#};

    const GROUPED_SOURCE: &str = indoc! {r#"
        fn outer() {
            alpha();
            beta();
            gamma();
        }
    "#};

    #[gpui::test]
    fn test_grouped_match_emits_one_runnable_per_run_item(cx: &mut TestAppContext) {
        let resolver: Arc<dyn RunnableResolver> = Arc::new(FirstRunResolver);
        let runnables = collect_runnables(
            cx,
            GROUPED_SOURCE,
            GROUPED_QUERY,
            Some(resolver),
            0..GROUPED_SOURCE.len(),
        );

        let run_texts: Vec<String> = runnables
            .iter()
            .map(|range| GROUPED_SOURCE[range.run_range.clone()].to_string())
            .collect();
        assert_eq!(run_texts, vec!["alpha", "beta", "gamma"]);

        for range in &runnables {
            assert_eq!(
                range.extra_captures.get("_outer").map(String::as_str),
                Some("outer")
            );
        }
    }

    #[gpui::test]
    fn test_grouped_match_offset_range_filters_groups(cx: &mut TestAppContext) {
        let resolver: Arc<dyn RunnableResolver> = Arc::new(FirstRunResolver);
        let beta_offset = GROUPED_SOURCE.find("beta()").expect("missing beta()");
        let runnables = collect_runnables(
            cx,
            GROUPED_SOURCE,
            GROUPED_QUERY,
            Some(resolver),
            beta_offset..beta_offset + "beta".len(),
        );

        let run_texts: Vec<String> = runnables
            .iter()
            .map(|range| GROUPED_SOURCE[range.run_range.clone()].to_string())
            .collect();
        assert_eq!(run_texts, vec!["beta"]);
    }
}
