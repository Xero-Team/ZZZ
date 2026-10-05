use std::{collections::BTreeMap, fmt, num::NonZeroUsize, time::Duration};

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
    use std::collections::BTreeSet;

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
