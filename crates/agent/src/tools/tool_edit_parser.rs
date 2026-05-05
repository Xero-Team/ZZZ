use smallvec::SmallVec;

use super::{EditOperation as Edit, PartialEditOperation as PartialEdit};

/// Events emitted by `ToolEditParser` as tool call input streams in.
#[derive(Debug, PartialEq, Eq)]
pub enum ToolEditEvent {
    /// A chunk of `old_text` for an edit operation.
    OldTextChunk {
        edit_index: usize,
        chunk: String,
        done: bool,
    },
    /// A chunk of `new_text` for an edit operation.
    NewTextChunk {
        edit_index: usize,
        chunk: String,
        done: bool,
    },
    /// A chunk of content for write/overwrite mode.
    ContentChunk { chunk: String },
}

#[derive(Default, Debug)]
struct EditStreamState {
    old_text_emitted_len: usize,
    old_text_done: bool,
    new_text_emitted_len: usize,
    new_text_done: bool,
}

#[derive(Default, Debug)]
pub struct ToolEditParser {
    edit_states: Vec<EditStreamState>,
    content_emitted_len: usize,
}

impl ToolEditParser {
    pub fn push_edits(&mut self, edits: &[PartialEdit]) -> SmallVec<[ToolEditEvent; 4]> {
        let mut events = SmallVec::new();

        for (index, partial) in edits.iter().enumerate() {
            if index >= self.edit_states.len() {
                if let Some(previous) = self.finalize_previous_edit(index) {
                    events.extend(previous);
                }
                self.edit_states.push(EditStreamState::default());
            }

            let state = &mut self.edit_states[index];

            if let Some(old_text) = &partial.old_text
                && !state.old_text_done
            {
                if partial.new_text.is_some() {
                    let start = state.old_text_emitted_len.min(old_text.len());
                    let chunk = normalize_done_chunk(old_text[start..].to_string());
                    state.old_text_done = true;
                    state.old_text_emitted_len = old_text.len();
                    events.push(ToolEditEvent::OldTextChunk {
                        edit_index: index,
                        chunk,
                        done: true,
                    });
                } else {
                    let safe_end = safe_emit_end_for_edit_text(old_text);

                    if safe_end > state.old_text_emitted_len {
                        let chunk = old_text[state.old_text_emitted_len..safe_end].to_string();
                        state.old_text_emitted_len = safe_end;
                        events.push(ToolEditEvent::OldTextChunk {
                            edit_index: index,
                            chunk,
                            done: false,
                        });
                    }
                }
            }

            if let Some(new_text) = &partial.new_text
                && !state.new_text_done
            {
                let safe_end = safe_emit_end_for_edit_text(new_text);

                if safe_end > state.new_text_emitted_len {
                    let chunk = new_text[state.new_text_emitted_len..safe_end].to_string();
                    state.new_text_emitted_len = safe_end;
                    events.push(ToolEditEvent::NewTextChunk {
                        edit_index: index,
                        chunk,
                        done: false,
                    });
                }
            }
        }

        events
    }

    pub fn push_content(&mut self, content: &str) -> SmallVec<[ToolEditEvent; 1]> {
        let mut events = SmallVec::new();

        let safe_end = safe_emit_end(content);
        if safe_end > self.content_emitted_len {
            let chunk = content[self.content_emitted_len..safe_end].to_string();
            self.content_emitted_len = safe_end;
            events.push(ToolEditEvent::ContentChunk { chunk });
        }

        events
    }

    pub fn finalize_edits(&mut self, edits: &[Edit]) -> SmallVec<[ToolEditEvent; 4]> {
        let mut events = SmallVec::new();

        for (index, edit) in edits.iter().enumerate() {
            if index >= self.edit_states.len() {
                if let Some(previous) = self.finalize_previous_edit(index) {
                    events.extend(previous);
                }
                self.edit_states.push(EditStreamState::default());
            }

            let state = &mut self.edit_states[index];

            if !state.old_text_done {
                let start = state.old_text_emitted_len.min(edit.old_text.len());
                let chunk = normalize_done_chunk(edit.old_text[start..].to_string());
                state.old_text_done = true;
                state.old_text_emitted_len = edit.old_text.len();
                events.push(ToolEditEvent::OldTextChunk {
                    edit_index: index,
                    chunk,
                    done: true,
                });
            }

            if !state.new_text_done {
                let start = state.new_text_emitted_len.min(edit.new_text.len());
                let chunk = normalize_done_chunk(edit.new_text[start..].to_string());
                state.new_text_done = true;
                state.new_text_emitted_len = edit.new_text.len();
                events.push(ToolEditEvent::NewTextChunk {
                    edit_index: index,
                    chunk,
                    done: true,
                });
            }
        }

        events
    }

    pub fn finalize_content(&mut self, content: &str) -> SmallVec<[ToolEditEvent; 1]> {
        let mut events = SmallVec::new();

        let start = self.content_emitted_len.min(content.len());
        if content.len() > start {
            let chunk = content[start..].to_string();
            self.content_emitted_len = content.len();
            events.push(ToolEditEvent::ContentChunk { chunk });
        }

        events
    }

    fn finalize_previous_edit(&mut self, new_index: usize) -> Option<SmallVec<[ToolEditEvent; 2]>> {
        if new_index == 0 || self.edit_states.is_empty() {
            return None;
        }

        let previous_index = new_index - 1;
        if previous_index >= self.edit_states.len() {
            return None;
        }

        let state = &mut self.edit_states[previous_index];
        let mut events = SmallVec::new();

        if !state.old_text_done {
            state.old_text_done = true;
            events.push(ToolEditEvent::OldTextChunk {
                edit_index: previous_index,
                chunk: String::new(),
                done: true,
            });
        }

        if !state.new_text_done {
            state.new_text_done = true;
            events.push(ToolEditEvent::NewTextChunk {
                edit_index: previous_index,
                chunk: String::new(),
                done: true,
            });
        }

        Some(events)
    }
}

fn safe_emit_end(text: &str) -> usize {
    if text.as_bytes().last() == Some(&b'\\') {
        text.len() - 1
    } else {
        text.len()
    }
}

fn safe_emit_end_for_edit_text(text: &str) -> usize {
    let safe_end = safe_emit_end(text);
    if safe_end > 0 && text.as_bytes()[safe_end - 1] == b'\n' {
        safe_end - 1
    } else {
        safe_end
    }
}

fn normalize_done_chunk(mut chunk: String) -> String {
    if chunk.ends_with('\\') {
        chunk.pop();
    }
    if chunk.ends_with('\n') {
        chunk.pop();
    }
    chunk
}
