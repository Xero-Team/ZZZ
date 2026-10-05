use crate::{
    CollisionPolicy, CopyOptions, CreateDirOptions, CreateDisposition, DirPageRequest, FileAccess,
    OpenOptions, OperationContext, ProviderPath, RemoveKind, RemoveOptions, RenameOptions,
    VfsError, VfsErrorCode, VfsProvider, WatchDepth, WatchRequest, WriteAtOptions,
};
use futures::StreamExt as _;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    num::{NonZeroU32, NonZeroUsize},
    sync::Arc,
    time::Duration,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum TransportDirection {
    ClientToServer,
    ServerToClient,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameOutcome {
    Deliver,
    Drop,
    Disconnect,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameFault {
    pub direction: TransportDirection,
    pub sequence: u64,
    pub delay: Duration,
    pub maximum_chunk_size: Option<NonZeroUsize>,
    pub outcome: FrameOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransportEvent {
    Chunk {
        direction: TransportDirection,
        sequence: u64,
        chunk_index: usize,
        is_last: bool,
        delay: Duration,
        bytes: Vec<u8>,
    },
    Dropped {
        direction: TransportDirection,
        sequence: u64,
        delay: Duration,
    },
    Disconnected {
        direction: TransportDirection,
        sequence: u64,
        delay: Duration,
    },
}

#[derive(Debug, Eq, PartialEq)]
pub struct DuplicateFrameFault {
    pub direction: TransportDirection,
    pub sequence: u64,
}

impl fmt::Display for DuplicateFrameFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "duplicate fault for {:?} frame {}",
            self.direction, self.sequence
        )
    }
}

impl std::error::Error for DuplicateFrameFault {}

pub struct FaultInjectingTransport {
    faults: BTreeMap<(TransportDirection, u64), FrameFault>,
    next_client_to_server_sequence: u64,
    next_server_to_client_sequence: u64,
    disconnected: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderConformanceError {
    #[error(transparent)]
    Provider(#[from] VfsError),
    #[error("provider conformance assertion failed: {0}")]
    Assertion(&'static str),
    #[error("provider watch stream ended before delivering an event")]
    WatchEnded,
}

pub async fn run_provider_conformance(
    provider: Arc<dyn VfsProvider>,
) -> Result<(), ProviderConformanceError> {
    let encoding = provider.descriptor().path_encoding;
    let root = ProviderPath::root(encoding);
    let directory = ProviderPath::from_byte_components(encoding, [b"workspace".as_slice()])
        .map_err(|_| ProviderConformanceError::Assertion("fixture path must be valid"))?;
    let source = ProviderPath::from_byte_components(
        encoding,
        [b"workspace".as_slice(), b"source.bin".as_slice()],
    )
    .map_err(|_| ProviderConformanceError::Assertion("fixture path must be valid"))?;
    let renamed = ProviderPath::from_byte_components(
        encoding,
        [b"workspace".as_slice(), b"renamed.bin".as_slice()],
    )
    .map_err(|_| ProviderConformanceError::Assertion("fixture path must be valid"))?;
    let copied = ProviderPath::from_byte_components(
        encoding,
        [b"workspace".as_slice(), b"copied.bin".as_slice()],
    )
    .map_err(|_| ProviderConformanceError::Assertion("fixture path must be valid"))?;

    let mut watch = provider
        .watch(WatchRequest {
            path: root.clone(),
            depth: WatchDepth::Recursive,
            resume_after_sequence: None,
            context: OperationContext::default(),
        })
        .await?;
    provider
        .create_dir(&directory, CreateDirOptions::default())
        .await?;
    let file = provider
        .open(
            &source,
            OpenOptions {
                access: FileAccess::ReadWrite,
                create: CreateDisposition::CreateNew,
                expected_version: None,
                context: OperationContext::default(),
            },
        )
        .await?;
    file.write_at(0, b"hello", WriteAtOptions::default())
        .await?;
    file.write_at(5, b" world", WriteAtOptions::default())
        .await?;

    let mut contents = vec![0; 11];
    let read = file
        .read_at(0, &mut contents, OperationContext::default())
        .await?;
    ensure(read == 11, "positioned read length")?;
    ensure(contents == b"hello world", "positioned read bytes")?;
    let eof = file
        .read_at(100, &mut contents, OperationContext::default())
        .await?;
    ensure(eof == 0, "read beyond EOF")?;

    let metadata = provider.stat(&source, Default::default()).await?;
    let stale_write = file
        .write_at(
            0,
            b"stale",
            WriteAtOptions {
                expected_version: Some(crate::VfsVersion::new(vec![0xff])),
                context: OperationContext::default(),
            },
        )
        .await;
    ensure(
        stale_write
            .as_ref()
            .err()
            .is_some_and(|error| error.code() == VfsErrorCode::StaleVersion),
        "stale write rejection",
    )?;

    file.set_len(32, WriteAtOptions::default()).await?;
    let mut sparse_tail = vec![0xff; 21];
    let sparse_read = file
        .read_at(11, &mut sparse_tail, OperationContext::default())
        .await?;
    ensure(sparse_read == 21, "sparse read length")?;
    ensure(
        sparse_tail.iter().all(|byte| *byte == 0),
        "sparse bytes are zero-filled",
    )?;

    provider
        .rename(&source, &renamed, RenameOptions::default())
        .await?;
    let renamed_metadata = provider.stat(&renamed, Default::default()).await?;
    ensure(
        renamed_metadata.provider_file_key == metadata.provider_file_key,
        "rename preserves provider file key",
    )?;
    provider
        .copy(
            &renamed,
            &copied,
            CopyOptions {
                collision: CollisionPolicy::Fail,
                ..Default::default()
            },
        )
        .await?;
    let copied_metadata = provider.stat(&copied, Default::default()).await?;
    ensure(
        copied_metadata.provider_file_key != renamed_metadata.provider_file_key,
        "copy creates a new provider file key",
    )?;

    let mut listed_paths = BTreeSet::new();
    let mut cursor = None;
    loop {
        let page = provider
            .read_dir(
                &directory,
                DirPageRequest {
                    cursor,
                    limit: NonZeroU32::MIN,
                    context: OperationContext::default(),
                },
            )
            .await?;
        listed_paths.extend(page.entries.into_iter().map(|entry| entry.path));
        let Some(next_cursor) = page.next_cursor else {
            break;
        };
        cursor = Some(next_cursor);
    }
    ensure(
        listed_paths == BTreeSet::from([renamed.clone(), copied.clone()]),
        "paged directory listing",
    )?;

    let first_batch = watch
        .next()
        .await
        .ok_or(ProviderConformanceError::WatchEnded)??;
    ensure(
        first_batch.first_sequence > 0,
        "watch sequence starts above zero",
    )?;
    ensure(
        first_batch.last_sequence >= first_batch.first_sequence,
        "watch sequence range",
    )?;

    let cancellation = crate::CancellationToken::default();
    cancellation.cancel();
    let cancelled = provider
        .stat(
            &renamed,
            crate::StatOptions {
                symbolic_link_mode: crate::SymbolicLinkMode::DoNotFollow,
                context: OperationContext {
                    operation_id: crate::OperationId::new(99),
                    cancellation,
                },
            },
        )
        .await;
    ensure(
        cancelled
            .as_ref()
            .err()
            .is_some_and(|error| error.code() == VfsErrorCode::Cancelled),
        "cancellation is typed",
    )?;

    let native_path = provider
        .native_path(&renamed, OperationContext::default())
        .await;
    if provider.capabilities().links.native_path.is_supported() {
        ensure(native_path.is_ok(), "supported native path mapping")?;
    } else {
        ensure(
            native_path
                .as_ref()
                .err()
                .is_some_and(|error| error.code() == VfsErrorCode::Unsupported),
            "unsupported operation is typed",
        )?;
    }

    provider
        .remove(
            &copied,
            RemoveOptions {
                kind: RemoveKind::File,
                expected_version: None,
                context: OperationContext::default(),
            },
        )
        .await?;
    provider
        .remove(
            &renamed,
            RemoveOptions {
                kind: RemoveKind::File,
                expected_version: None,
                context: OperationContext::default(),
            },
        )
        .await?;
    provider
        .remove(
            &directory,
            RemoveOptions {
                kind: RemoveKind::EmptyDirectory,
                expected_version: None,
                context: OperationContext::default(),
            },
        )
        .await?;
    Ok(())
}

fn ensure(condition: bool, message: &'static str) -> Result<(), ProviderConformanceError> {
    if condition {
        Ok(())
    } else {
        Err(ProviderConformanceError::Assertion(message))
    }
}

impl FaultInjectingTransport {
    pub fn new(faults: impl IntoIterator<Item = FrameFault>) -> Result<Self, DuplicateFrameFault> {
        let mut faults_by_frame = BTreeMap::new();
        for fault in faults {
            let key = (fault.direction, fault.sequence);
            if faults_by_frame.insert(key, fault).is_some() {
                return Err(DuplicateFrameFault {
                    direction: key.0,
                    sequence: key.1,
                });
            }
        }

        Ok(Self {
            faults: faults_by_frame,
            next_client_to_server_sequence: 0,
            next_server_to_client_sequence: 0,
            disconnected: false,
        })
    }

    pub fn transmit(
        &mut self,
        direction: TransportDirection,
        bytes: Vec<u8>,
    ) -> Vec<TransportEvent> {
        let sequence = match direction {
            TransportDirection::ClientToServer => {
                let sequence = self.next_client_to_server_sequence;
                self.next_client_to_server_sequence += 1;
                sequence
            }
            TransportDirection::ServerToClient => {
                let sequence = self.next_server_to_client_sequence;
                self.next_server_to_client_sequence += 1;
                sequence
            }
        };

        if self.disconnected {
            self.faults.remove(&(direction, sequence));
            return vec![TransportEvent::Disconnected {
                direction,
                sequence,
                delay: Duration::ZERO,
            }];
        }

        let fault = self.faults.remove(&(direction, sequence));
        let (delay, maximum_chunk_size, outcome) = match fault {
            Some(fault) => (fault.delay, fault.maximum_chunk_size, fault.outcome),
            None => (Duration::ZERO, None, FrameOutcome::Deliver),
        };

        match outcome {
            FrameOutcome::Drop => vec![TransportEvent::Dropped {
                direction,
                sequence,
                delay,
            }],
            FrameOutcome::Disconnect => {
                self.disconnected = true;
                vec![TransportEvent::Disconnected {
                    direction,
                    sequence,
                    delay,
                }]
            }
            FrameOutcome::Deliver => {
                let maximum_chunk_size = match maximum_chunk_size {
                    Some(maximum_chunk_size) => maximum_chunk_size.get(),
                    None => bytes.len().max(1),
                };
                if bytes.is_empty() {
                    return vec![TransportEvent::Chunk {
                        direction,
                        sequence,
                        chunk_index: 0,
                        is_last: true,
                        delay,
                        bytes,
                    }];
                }

                let chunk_count = bytes.len().div_ceil(maximum_chunk_size);
                bytes
                    .chunks(maximum_chunk_size)
                    .enumerate()
                    .map(|(chunk_index, chunk)| TransportEvent::Chunk {
                        direction,
                        sequence,
                        chunk_index,
                        is_last: chunk_index + 1 == chunk_count,
                        delay: if chunk_index == 0 {
                            delay
                        } else {
                            Duration::ZERO
                        },
                        bytes: chunk.to_vec(),
                    })
                    .collect()
            }
        }
    }

    pub fn reconnect(&mut self) {
        self.disconnected = false;
    }

    pub fn remaining_fault_count(&self) -> usize {
        self.faults.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct PathCorpus {
        schema_version: u32,
        seed: u64,
        cases: Vec<PathCorpusCase>,
    }

    #[derive(Debug, Deserialize)]
    struct PathCorpusCase {
        id: String,
        encoding: String,
        form: String,
        root_kind: String,
        components_hex: Vec<String>,
        expected: String,
    }

    #[test]
    fn fault_injection_is_directional_and_deterministic() {
        let Some(maximum_chunk_size) = NonZeroUsize::new(2) else {
            panic!("two must be non-zero");
        };
        let Ok(mut transport) = FaultInjectingTransport::new([
            FrameFault {
                direction: TransportDirection::ClientToServer,
                sequence: 0,
                delay: Duration::from_millis(25),
                maximum_chunk_size: Some(maximum_chunk_size),
                outcome: FrameOutcome::Deliver,
            },
            FrameFault {
                direction: TransportDirection::ServerToClient,
                sequence: 0,
                delay: Duration::ZERO,
                maximum_chunk_size: None,
                outcome: FrameOutcome::Drop,
            },
            FrameFault {
                direction: TransportDirection::ClientToServer,
                sequence: 1,
                delay: Duration::ZERO,
                maximum_chunk_size: None,
                outcome: FrameOutcome::Disconnect,
            },
        ]) else {
            panic!("the fault script must not contain duplicate frame keys");
        };

        assert_eq!(
            transport.transmit(TransportDirection::ClientToServer, b"abcde".to_vec()),
            vec![
                TransportEvent::Chunk {
                    direction: TransportDirection::ClientToServer,
                    sequence: 0,
                    chunk_index: 0,
                    is_last: false,
                    delay: Duration::from_millis(25),
                    bytes: b"ab".to_vec(),
                },
                TransportEvent::Chunk {
                    direction: TransportDirection::ClientToServer,
                    sequence: 0,
                    chunk_index: 1,
                    is_last: false,
                    delay: Duration::ZERO,
                    bytes: b"cd".to_vec(),
                },
                TransportEvent::Chunk {
                    direction: TransportDirection::ClientToServer,
                    sequence: 0,
                    chunk_index: 2,
                    is_last: true,
                    delay: Duration::ZERO,
                    bytes: b"e".to_vec(),
                },
            ]
        );
        assert_eq!(
            transport.transmit(TransportDirection::ServerToClient, b"response".to_vec()),
            vec![TransportEvent::Dropped {
                direction: TransportDirection::ServerToClient,
                sequence: 0,
                delay: Duration::ZERO,
            }]
        );
        assert_eq!(
            transport.transmit(TransportDirection::ClientToServer, b"commit".to_vec()),
            vec![TransportEvent::Disconnected {
                direction: TransportDirection::ClientToServer,
                sequence: 1,
                delay: Duration::ZERO,
            }]
        );
        assert_eq!(
            transport.transmit(TransportDirection::ServerToClient, b"late".to_vec()),
            vec![TransportEvent::Disconnected {
                direction: TransportDirection::ServerToClient,
                sequence: 1,
                delay: Duration::ZERO,
            }]
        );

        transport.reconnect();
        assert!(matches!(
            transport
                .transmit(TransportDirection::ServerToClient, Vec::new())
                .as_slice(),
            [TransportEvent::Chunk {
                sequence: 2,
                is_last: true,
                bytes,
                ..
            }] if bytes.is_empty()
        ));
        assert_eq!(transport.remaining_fault_count(), 0);
    }

    #[test]
    fn duplicate_faults_are_rejected() {
        let fault = FrameFault {
            direction: TransportDirection::ClientToServer,
            sequence: 7,
            delay: Duration::ZERO,
            maximum_chunk_size: None,
            outcome: FrameOutcome::Drop,
        };
        let result = FaultInjectingTransport::new([fault.clone(), fault]);
        assert_eq!(
            result.err(),
            Some(DuplicateFrameFault {
                direction: TransportDirection::ClientToServer,
                sequence: 7,
            })
        );
    }

    #[test]
    fn path_corpus_has_fixed_cross_platform_coverage() {
        let parsed =
            serde_json::from_str::<PathCorpus>(include_str!("../test_data/path_corpus.json"));
        let Ok(corpus) = parsed else {
            panic!("path corpus must be valid JSON: {parsed:?}");
        };

        assert_eq!(corpus.schema_version, 1);
        assert_eq!(corpus.seed, 1_592_614_637);
        assert!(corpus.cases.len() >= 14);

        let mut ids = BTreeSet::new();
        let mut encodings = BTreeSet::new();
        let mut forms = BTreeSet::new();
        let mut root_kinds = BTreeSet::new();
        let mut expectations = BTreeSet::new();
        for case in &corpus.cases {
            assert!(
                ids.insert(case.id.as_str()),
                "duplicate case id: {}",
                case.id
            );
            encodings.insert(case.encoding.as_str());
            forms.insert(case.form.as_str());
            root_kinds.insert(case.root_kind.as_str());
            expectations.insert(case.expected.as_str());
            for component in &case.components_hex {
                assert!(component.len() % 2 == 0, "odd hex length in {}", case.id);
                assert!(
                    component.bytes().all(|byte| byte.is_ascii_hexdigit()),
                    "non-hex component in {}",
                    case.id
                );
            }
        }

        assert_eq!(
            encodings,
            BTreeSet::from(["portable_utf8", "unix_bytes", "windows_wtf8"])
        );
        assert_eq!(
            forms,
            BTreeSet::from(["archive_member", "native", "provider_relative"])
        );
        assert!(root_kinds.contains("windows_drive"));
        assert!(root_kinds.contains("windows_unc"));
        assert!(root_kinds.contains("windows_verbatim_drive"));
        assert_eq!(expectations, BTreeSet::from(["invalid", "valid"]));
    }
}
