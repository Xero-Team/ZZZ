//! Size ceilings for the messages the remote-editing path puts on a socket.
//!
//! Remote development encodes proto messages with `prost`, compresses the frame
//! with zstd, and sends it over a WebSocket on every keystroke, save, worktree
//! scan and language-server response. These are ordinary unit tests, so the
//! existing test job catches a field added to a hot message; no new workflow and
//! no new runtime dependency is involved.
//!
//! Terminal bytes do not cross this protocol (ACP owns the terminal), so the
//! scenarios below cover the editing path instead: an edit, a save, a reload, a
//! file-metadata update, a worktree scan batch, a repository status batch, an
//! LSP response and a buffer synchronization.
//!
//! A ceiling that fails is a decision point, not an accident: either the field is
//! worth its bytes and the number moves with a comment explaining why, or the
//! field does not belong on a message this hot.

use crate::{
    BufferReloaded, BufferSaved, BufferVersion, Entry, EnvelopedMessage, ExactPathV2, File,
    GetHoverResponse, GitFileStatus, GitStatus, HoverBlock, LspQueryResponse, LspResponse, Message,
    MountIdV2, NativePathRootKindV2, NativePathRootV2, Operation, PathEncodingV2, ProviderPathV2,
    Range, StatusEntry, SynchronizeBuffers, Timestamp, UpdateBuffer, UpdateBufferFile,
    UpdateRepository, UpdateWorktree, VectorClockEntry, VfsPathV2, git_file_status, lsp_response,
    operation,
};

/// The compression level `MessageStream::write` uses outside tests.
///
/// Kept in step with `crates/rpc/src/message_stream.rs`; a test build compresses
/// at `-7`, which is not what a user's remote session sees.
const WIRE_COMPRESSION_LEVEL: i32 = 4;

/// One hot message: a representative payload plus the ceilings it must stay under.
struct Scenario {
    /// The `EnvelopedMessage::NAME` of the measured message.
    message: &'static str,
    /// Ceiling on `encode_to_vec().len()` for the payload.
    encoded: usize,
    /// Ceiling on the zstd frame the stream sends.
    compressed: usize,
    /// What a failure of this ceiling means.
    rationale: &'static str,
    /// The payload the ceilings were measured against.
    build: fn() -> Vec<u8>,
}

/// Every message the remote-editing path sends, with its measured ceilings.
///
/// This table is the forcing function: a message that is not listed here has no
/// coverage, and adding a field to a listed message trips its ceiling. Each
/// number is the measured size plus a small margin, so an added field or a wider
/// encoding fails instead of quietly costing bytes for every remote user.
const SCENARIOS: &[Scenario] = &[
    Scenario {
        message: UpdateBuffer::NAME,
        encoded: 33,
        compressed: 44,
        rationale: "one keystroke; this is paid on every character the user types",
        build: build_update_buffer,
    },
    Scenario {
        message: BufferSaved::NAME,
        encoded: 29,
        compressed: 40,
        rationale: "one save; grows with the saved version vector and the mtime encoding",
        build: build_buffer_saved,
    },
    Scenario {
        message: BufferReloaded::NAME,
        encoded: 29,
        compressed: 40,
        rationale: "one reload after the file changed on disk",
        build: build_buffer_reloaded,
    },
    Scenario {
        message: UpdateBufferFile::NAME,
        encoded: 84,
        compressed: 92,
        rationale: "one file-metadata update, sent when a buffer opens or changes on disk",
        build: build_update_buffer_file,
    },
    Scenario {
        message: UpdateWorktree::NAME,
        encoded: 2450,
        compressed: 260,
        rationale: "a 32-entry worktree scan batch",
        build: build_update_worktree,
    },
    Scenario {
        message: UpdateRepository::NAME,
        encoded: 1900,
        compressed: 100,
        rationale: "a 32-file git status batch",
        build: build_update_repository,
    },
    Scenario {
        message: LspQueryResponse::NAME,
        encoded: 740,
        compressed: 120,
        rationale: "one hover response carrying documentation",
        build: build_lsp_query_response,
    },
    Scenario {
        message: SynchronizeBuffers::NAME,
        encoded: 104,
        compressed: 56,
        rationale: "opening a project with 8 buffers already open",
        build: build_synchronize_buffers,
    },
];

fn version(entries: u8) -> Vec<VectorClockEntry> {
    (0..entries)
        .map(|replica_id| VectorClockEntry {
            replica_id: u32::from(replica_id),
            timestamp: u32::from(replica_id),
        })
        .collect()
}

fn mtime() -> Option<Timestamp> {
    Some(Timestamp {
        seconds: 1_700_000_000,
        nanos: 123_456_789,
    })
}

fn vfs_path(components: &[&str]) -> Option<VfsPathV2> {
    Some(VfsPathV2 {
        mount_id: Some(MountIdV2 { value: 1 }),
        relative_path: Some(ExactPathV2 {
            encoding: PathEncodingV2::UnixBytes as i32,
            root: Some(NativePathRootV2 {
                kind: NativePathRootKindV2::Posix as i32,
                absolute: true,
                ..Default::default()
            }),
            components: components
                .iter()
                .map(|component| component.as_bytes().to_vec())
                .collect(),
        }),
    })
}

fn anchor() -> crate::Anchor {
    crate::Anchor {
        replica_id: 0,
        timestamp: 42,
        offset: 1_200,
        bias: crate::Bias::Right as i32,
        buffer_id: Some(2),
    }
}

fn build_update_buffer() -> Vec<u8> {
    UpdateBuffer {
        project_id: 1,
        buffer_id: 2,
        operations: vec![Operation {
            variant: Some(operation::Variant::Edit(operation::Edit {
                replica_id: 0,
                lamport_timestamp: 7,
                version: version(2),
                ranges: vec![Range {
                    start: 1_200,
                    end: 1_200,
                }],
                new_text: vec!["x".to_owned()],
            })),
        }],
    }
    .encode_to_vec()
}

fn build_buffer_saved() -> Vec<u8> {
    BufferSaved {
        project_id: 1,
        buffer_id: 2,
        version: version(2),
        mtime: mtime(),
    }
    .encode_to_vec()
}

fn build_buffer_reloaded() -> Vec<u8> {
    BufferReloaded {
        project_id: 1,
        buffer_id: 2,
        version: version(2),
        mtime: mtime(),
        line_ending: crate::LineEnding::Unix as i32,
    }
    .encode_to_vec()
}

fn build_update_buffer_file() -> Vec<u8> {
    UpdateBufferFile {
        project_id: 1,
        buffer_id: 2,
        file: Some(File {
            worktree_id: 3,
            entry_id: Some(4),
            mtime: mtime(),
            is_deleted: false,
            is_historic: false,
            vfs_path: vfs_path(&["crates", "proto", "src", "proto.rs"]),
            resource_id: None,
        }),
    }
    .encode_to_vec()
}

fn build_update_worktree() -> Vec<u8> {
    let entry = |id: u64| Entry {
        id,
        is_dir: false,
        inode: id,
        mtime: mtime(),
        size: Some(4_096),
        vfs_path: vfs_path(&["crates", "proto", "src", "proto.rs"]),
        ..Default::default()
    };

    UpdateWorktree {
        project_id: 1,
        worktree_id: 3,
        root_name: "zzz".to_owned(),
        updated_entries: (0..32).map(entry).collect(),
        scan_id: 9,
        is_last_update: true,
        ..Default::default()
    }
    .encode_to_vec()
}

fn build_update_repository() -> Vec<u8> {
    let status = StatusEntry {
        simple_status: GitStatus::Modified as i32,
        status: Some(GitFileStatus {
            variant: Some(git_file_status::Variant::Tracked(
                git_file_status::Tracked {
                    index_status: GitStatus::Modified as i32,
                    worktree_status: GitStatus::Modified as i32,
                },
            )),
        }),
        diff_stat_added: Some(12),
        diff_stat_deleted: Some(3),
        repo_path_v2: Some(ProviderPathV2 {
            path: vfs_path(&["crates", "proto", "src", "proto.rs"])
                .and_then(|path| path.relative_path),
        }),
    };

    UpdateRepository {
        project_id: 1,
        id: 5,
        updated_statuses: (0..32).map(|_| status.clone()).collect(),
        scan_id: 11,
        is_last_update: true,
        ..Default::default()
    }
    .encode_to_vec()
}

fn build_lsp_query_response() -> Vec<u8> {
    LspQueryResponse {
        project_id: 1,
        lsp_request_id: 13,
        responses: vec![LspResponse {
            server_id: 17,
            response: Some(lsp_response::Response::GetHoverResponse(GetHoverResponse {
                start: Some(anchor()),
                end: Some(anchor()),
                contents: vec![HoverBlock {
                    text: "// Documentation for the changed symbol.\n".repeat(16),
                    language: Some("rust".to_owned()),
                    is_markdown: true,
                }],
            })),
        }],
    }
    .encode_to_vec()
}

fn build_synchronize_buffers() -> Vec<u8> {
    SynchronizeBuffers {
        project_id: 1,
        buffers: (0..8)
            .map(|id| BufferVersion {
                id,
                version: version(2),
            })
            .collect(),
    }
    .encode_to_vec()
}

fn compressed_len(payload: &[u8]) -> usize {
    zstd::stream::encode_all(payload, WIRE_COMPRESSION_LEVEL)
        .expect("in-memory zstd encoding cannot fail")
        .len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payloads_fit_their_budgets() {
        for scenario in SCENARIOS {
            let payload = (scenario.build)();
            let encoded = payload.len();
            let compressed = compressed_len(&payload);

            assert!(
                encoded <= scenario.encoded,
                "{} grew to {encoded} bytes, past its {} byte budget: {}",
                scenario.message,
                scenario.encoded,
                scenario.rationale
            );
            assert!(
                compressed <= scenario.compressed,
                "{} compresses to {compressed} bytes, past its {} byte budget: {}",
                scenario.message,
                scenario.compressed,
                scenario.rationale
            );
        }
    }

    #[test]
    fn scenario_names_are_unique() {
        for (index, scenario) in SCENARIOS.iter().enumerate() {
            assert!(
                SCENARIOS[..index]
                    .iter()
                    .all(|earlier| earlier.message != scenario.message),
                "{} is budgeted twice",
                scenario.message
            );
        }
    }
}
