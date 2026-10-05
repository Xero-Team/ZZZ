use crate::{
    ExactPathV2, MountIdV2, NativePathRootKindV2, NativePathRootV2, NativePathV2, PathEncodingV2,
    ProviderPathV2, ResourceIdV2, VfsPathComponentV2, VfsPathV2,
};
use vfs::{
    DisplayComponent, EntryName, ExactComponent, LookupKey, MountId, NativePath, NativePathRoot,
    PathEncoding, ProviderPath, ResourceId, VfsPath, WindowsDrive, WindowsDriveForm,
    WindowsUncForm,
};

#[derive(Debug, thiserror::Error)]
pub enum VfsWireError {
    #[error("unknown VFS path encoding {0}")]
    UnknownPathEncoding(i32),
    #[error("unknown native path root kind {0}")]
    UnknownNativeRootKind(i32),
    #[error("missing {0}")]
    MissingField(&'static str),
    #[error("provider path carries a native root")]
    ProviderPathHasNativeRoot,
    #[error("Windows drive root must contain exactly one ASCII byte")]
    InvalidWindowsDrive,
    #[error("native root contains fields that do not match its kind")]
    InvalidNativeRoot,
    #[error("invalid VFS protobuf payload")]
    InvalidProtobuf(#[from] crate::DecodeError),
    #[error(transparent)]
    InvalidPath(#[from] vfs::PathError),
}

impl ExactPathV2 {
    pub fn from_provider_path(path: &ProviderPath) -> Self {
        Self {
            encoding: encode_path_encoding(path.encoding()) as i32,
            root: Some(relative_root()),
            components: path
                .components()
                .map(|component| component.as_bytes().to_vec())
                .collect(),
        }
    }

    pub fn to_provider_path(&self) -> Result<ProviderPath, VfsWireError> {
        let encoding = decode_path_encoding(self.encoding)?;
        let root = self
            .root
            .as_ref()
            .ok_or(VfsWireError::MissingField("exact path root"))?;
        let root_kind = decode_root_kind(root.kind)?;
        if root_kind != NativePathRootKindV2::Relative || !root_has_no_payload_or_flags(root) {
            return Err(VfsWireError::ProviderPathHasNativeRoot);
        }
        ProviderPath::from_byte_components(encoding, &self.components).map_err(Into::into)
    }

    pub fn from_native_path(path: &NativePath) -> Self {
        Self {
            encoding: encode_path_encoding(path.provider_path().encoding()) as i32,
            root: Some(encode_native_root(path.root())),
            components: path
                .provider_path()
                .components()
                .map(|component| component.as_bytes().to_vec())
                .collect(),
        }
    }

    pub fn to_native_path(&self) -> Result<NativePath, VfsWireError> {
        let encoding = decode_path_encoding(self.encoding)?;
        let root = self
            .root
            .as_ref()
            .ok_or(VfsWireError::MissingField("exact path root"))?;
        let native_root = decode_native_root(root)?;
        let provider_path = ProviderPath::from_byte_components(encoding, &self.components)?;
        NativePath::new(native_root, provider_path).map_err(Into::into)
    }
}

impl MountIdV2 {
    pub fn from_mount_id(mount_id: MountId) -> Self {
        Self {
            value: mount_id.get(),
        }
    }

    pub fn to_mount_id(&self) -> MountId {
        MountId::new(self.value)
    }
}

impl ResourceIdV2 {
    pub fn from_resource_id(resource_id: ResourceId) -> Self {
        Self {
            mount_id: Some(MountIdV2::from_mount_id(resource_id.mount_id())),
            node_id: resource_id.node_id(),
            generation: resource_id.generation(),
        }
    }

    pub fn to_resource_id(&self) -> Result<ResourceId, VfsWireError> {
        let mount_id = self
            .mount_id
            .as_ref()
            .ok_or(VfsWireError::MissingField("resource mount id"))?
            .to_mount_id();
        Ok(ResourceId::new(mount_id, self.node_id, self.generation))
    }
}

impl ProviderPathV2 {
    pub fn from_provider_path(path: &ProviderPath) -> Self {
        Self {
            path: Some(ExactPathV2::from_provider_path(path)),
        }
    }

    pub fn to_provider_path(&self) -> Result<ProviderPath, VfsWireError> {
        self.path
            .as_ref()
            .ok_or(VfsWireError::MissingField("provider path"))?
            .to_provider_path()
    }
}

impl VfsPathV2 {
    pub fn from_vfs_path(path: &VfsPath) -> Self {
        Self {
            mount_id: Some(MountIdV2::from_mount_id(path.mount_id())),
            relative_path: Some(ExactPathV2::from_provider_path(path.provider_path())),
        }
    }

    pub fn to_vfs_path(&self) -> Result<VfsPath, VfsWireError> {
        let mount_id = self
            .mount_id
            .as_ref()
            .ok_or(VfsWireError::MissingField("VFS mount id"))?
            .to_mount_id();
        let path = self
            .relative_path
            .as_ref()
            .ok_or(VfsWireError::MissingField("VFS relative path"))?
            .to_provider_path()?;
        Ok(VfsPath::new(mount_id, path))
    }
}

impl NativePathV2 {
    pub fn from_native_path(path: &NativePath) -> Self {
        Self {
            path: Some(ExactPathV2::from_native_path(path)),
        }
    }

    pub fn to_native_path(&self) -> Result<NativePath, VfsWireError> {
        self.path
            .as_ref()
            .ok_or(VfsWireError::MissingField("native path"))?
            .to_native_path()
    }
}

impl VfsPathComponentV2 {
    pub fn from_entry_name(entry_name: &EntryName) -> Self {
        Self {
            encoding: encode_path_encoding(entry_name.exact().encoding()) as i32,
            exact: entry_name.exact().as_bytes().to_vec(),
            display: entry_name.display().as_str().to_owned(),
            lookup_key: entry_name.lookup_key().as_bytes().to_vec(),
        }
    }

    pub fn to_entry_name(&self) -> Result<EntryName, VfsWireError> {
        let encoding = decode_path_encoding(self.encoding)?;
        Ok(EntryName::new(
            ExactComponent::new(encoding, self.exact.clone())?,
            DisplayComponent::new(self.display.clone()),
            LookupKey::new(self.lookup_key.clone()),
        ))
    }
}

fn encode_path_encoding(encoding: PathEncoding) -> PathEncodingV2 {
    match encoding {
        PathEncoding::UnixBytes => PathEncodingV2::UnixBytes,
        PathEncoding::WindowsWtf8 => PathEncodingV2::WindowsWtf8,
        PathEncoding::PortableUtf8 => PathEncodingV2::PortableUtf8,
    }
}

fn decode_path_encoding(encoding: i32) -> Result<PathEncoding, VfsWireError> {
    match PathEncodingV2::try_from(encoding) {
        Ok(PathEncodingV2::UnixBytes) => Ok(PathEncoding::UnixBytes),
        Ok(PathEncodingV2::WindowsWtf8) => Ok(PathEncoding::WindowsWtf8),
        Ok(PathEncodingV2::PortableUtf8) => Ok(PathEncoding::PortableUtf8),
        Ok(PathEncodingV2::Unspecified) | Err(_) => {
            Err(VfsWireError::UnknownPathEncoding(encoding))
        }
    }
}

fn relative_root() -> NativePathRootV2 {
    NativePathRootV2 {
        kind: NativePathRootKindV2::Relative as i32,
        ..Default::default()
    }
}

fn encode_native_root(root: &NativePathRoot) -> NativePathRootV2 {
    match root {
        NativePathRoot::Relative => relative_root(),
        NativePathRoot::Posix => NativePathRootV2 {
            kind: NativePathRootKindV2::Posix as i32,
            ..Default::default()
        },
        NativePathRoot::WindowsRootRelative => NativePathRootV2 {
            kind: NativePathRootKindV2::WindowsRootRelative as i32,
            ..Default::default()
        },
        NativePathRoot::WindowsDrive { drive, form } => NativePathRootV2 {
            kind: NativePathRootKindV2::WindowsDrive as i32,
            drive: vec![drive.as_ascii()],
            absolute: form.is_absolute(),
            verbatim: form.is_verbatim(),
            ..Default::default()
        },
        NativePathRoot::WindowsUnc {
            server,
            share,
            form,
        } => NativePathRootV2 {
            kind: NativePathRootKindV2::WindowsUnc as i32,
            server: server.as_bytes().to_vec(),
            share: share.as_bytes().to_vec(),
            verbatim: *form == WindowsUncForm::Verbatim,
            ..Default::default()
        },
        NativePathRoot::WindowsDevice { device } => NativePathRootV2 {
            kind: NativePathRootKindV2::WindowsDevice as i32,
            namespace: device.as_bytes().to_vec(),
            ..Default::default()
        },
        NativePathRoot::WindowsVerbatim { namespace } => NativePathRootV2 {
            kind: NativePathRootKindV2::WindowsVerbatim as i32,
            namespace: namespace.as_bytes().to_vec(),
            verbatim: true,
            ..Default::default()
        },
    }
}

fn decode_native_root(root: &NativePathRootV2) -> Result<NativePathRoot, VfsWireError> {
    match decode_root_kind(root.kind)? {
        NativePathRootKindV2::Relative if root_has_no_payload_or_flags(root) => {
            Ok(NativePathRoot::Relative)
        }
        NativePathRootKindV2::Posix if root_has_no_payload_or_flags(root) => {
            Ok(NativePathRoot::Posix)
        }
        NativePathRootKindV2::WindowsRootRelative if root_has_no_payload_or_flags(root) => {
            Ok(NativePathRoot::WindowsRootRelative)
        }
        NativePathRootKindV2::WindowsDrive
            if root.server.is_empty() && root.share.is_empty() && root.namespace.is_empty() =>
        {
            let [drive] = root.drive.as_slice() else {
                return Err(VfsWireError::InvalidWindowsDrive);
            };
            Ok(NativePathRoot::WindowsDrive {
                drive: WindowsDrive::new(*drive).map_err(|_| VfsWireError::InvalidWindowsDrive)?,
                form: match (root.absolute, root.verbatim) {
                    (false, false) => WindowsDriveForm::Relative,
                    (true, false) => WindowsDriveForm::Absolute,
                    (false, true) => WindowsDriveForm::VerbatimRelative,
                    (true, true) => WindowsDriveForm::VerbatimAbsolute,
                },
            })
        }
        NativePathRootKindV2::WindowsUnc
            if root.drive.is_empty() && root.namespace.is_empty() && !root.absolute =>
        {
            Ok(NativePathRoot::WindowsUnc {
                server: ExactComponent::new(PathEncoding::WindowsWtf8, root.server.clone())?,
                share: ExactComponent::new(PathEncoding::WindowsWtf8, root.share.clone())?,
                form: if root.verbatim {
                    WindowsUncForm::Verbatim
                } else {
                    WindowsUncForm::Standard
                },
            })
        }
        NativePathRootKindV2::WindowsDevice
            if root.drive.is_empty()
                && root.server.is_empty()
                && root.share.is_empty()
                && !root.absolute
                && !root.verbatim =>
        {
            Ok(NativePathRoot::WindowsDevice {
                device: ExactComponent::new(PathEncoding::WindowsWtf8, root.namespace.clone())?,
            })
        }
        NativePathRootKindV2::WindowsVerbatim
            if root.drive.is_empty()
                && root.server.is_empty()
                && root.share.is_empty()
                && !root.absolute
                && root.verbatim =>
        {
            Ok(NativePathRoot::WindowsVerbatim {
                namespace: ExactComponent::new(PathEncoding::WindowsWtf8, root.namespace.clone())?,
            })
        }
        NativePathRootKindV2::Unspecified => Err(VfsWireError::UnknownNativeRootKind(root.kind)),
        _ => Err(VfsWireError::InvalidNativeRoot),
    }
}

fn decode_root_kind(kind: i32) -> Result<NativePathRootKindV2, VfsWireError> {
    match NativePathRootKindV2::try_from(kind) {
        Ok(NativePathRootKindV2::Unspecified) | Err(_) => {
            Err(VfsWireError::UnknownNativeRootKind(kind))
        }
        Ok(kind) => Ok(kind),
    }
}

fn root_has_no_payload_or_flags(root: &NativePathRootV2) -> bool {
    root_has_no_payload(root) && !root.absolute && !root.verbatim
}

fn root_has_no_payload(root: &NativePathRootV2) -> bool {
    root.drive.is_empty()
        && root.server.is_empty()
        && root.share.is_empty()
        && root.namespace.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message as _;
    use serde::Deserialize;
    use std::collections::BTreeSet;

    #[derive(Debug, Deserialize)]
    struct PathCorpus {
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
    fn fixed_path_corpus_round_trips_through_protobuf() {
        let parsed = serde_json::from_str::<PathCorpus>(include_str!(
            "../../vfs/test_data/path_corpus.json"
        ));
        let Ok(corpus) = parsed else {
            panic!("path corpus must parse: {parsed:?}");
        };
        let mut round_tripped = BTreeSet::new();
        let mut rejected = BTreeSet::new();

        for case in corpus.cases {
            let encoding = encoding_from_fixture(&case.encoding);
            let Ok(encoding) = encoding else {
                panic!("fixture encoding must be known: {case:?}");
            };
            let components = case
                .components_hex
                .iter()
                .map(|component| decode_hex(component))
                .collect::<Result<Vec<_>, _>>();
            let Ok(components) = components else {
                panic!("fixture components must be hex: {case:?}");
            };

            let result = match case.form.as_str() {
                "provider_relative" | "archive_member" => {
                    round_trip_provider_fixture(&case, encoding, components)
                }
                "native" => round_trip_native_fixture(&case, encoding, components),
                _ => panic!("unknown fixture form: {case:?}"),
            };

            if case.expected == "valid" {
                assert!(
                    result.is_ok(),
                    "valid fixture failed: {}: {result:?}",
                    case.id
                );
                round_tripped.insert(case.id);
            } else {
                assert!(result.is_err(), "invalid fixture passed: {}", case.id);
                rejected.insert(case.id);
            }
        }

        assert_eq!(round_tripped.len(), 10);
        assert_eq!(rejected.len(), 6);
    }

    #[test]
    fn resource_and_entry_name_wire_types_round_trip() {
        let resource_id = ResourceId::new(MountId::new(17), 42, 3);
        let wire_resource = ResourceIdV2::from_resource_id(resource_id);
        let round_tripped_resource = wire_resource.to_resource_id();
        let Ok(round_tripped_resource) = round_tripped_resource else {
            panic!("resource id must round trip: {round_tripped_resource:?}");
        };
        assert_eq!(round_tripped_resource, resource_id);

        let exact = ExactComponent::new(PathEncoding::UnixBytes, b"name".to_vec());
        let Ok(exact) = exact else {
            panic!("test component must be valid: {exact:?}");
        };
        let entry_name = EntryName::new(
            exact,
            DisplayComponent::new("Name"),
            LookupKey::new(b"name".to_vec()),
        );
        let wire_entry_name = VfsPathComponentV2::from_entry_name(&entry_name);
        assert_eq!(wire_entry_name.to_entry_name().ok(), Some(entry_name));
    }

    fn round_trip_provider_fixture(
        case: &PathCorpusCase,
        encoding: PathEncoding,
        components: Vec<Vec<u8>>,
    ) -> Result<(), VfsWireError> {
        if case.root_kind != "relative" {
            let wire = ProviderPathV2 {
                path: Some(ExactPathV2 {
                    encoding: encode_path_encoding(encoding) as i32,
                    root: Some(NativePathRootV2 {
                        kind: NativePathRootKindV2::WindowsDrive as i32,
                        drive: vec![b'C'],
                        absolute: true,
                        ..Default::default()
                    }),
                    components,
                }),
            };
            wire.to_provider_path()?;
            return Ok(());
        }

        let path = ProviderPath::from_byte_components(encoding, &components)?;
        let wire = ProviderPathV2::from_provider_path(&path);
        let decoded = protobuf_round_trip_provider_path(&wire)?;
        let round_tripped = decoded.to_provider_path()?;
        assert_eq!(round_tripped, path);
        Ok(())
    }

    fn round_trip_native_fixture(
        case: &PathCorpusCase,
        encoding: PathEncoding,
        mut components: Vec<Vec<u8>>,
    ) -> Result<(), VfsWireError> {
        if encoding != PathEncoding::WindowsWtf8 {
            return Err(VfsWireError::InvalidNativeRoot);
        }
        let root = match case.root_kind.as_str() {
            "windows_drive" | "windows_verbatim_drive" => {
                let drive = take_first_component(&mut components)?;
                let [drive] = drive.as_slice() else {
                    return Err(VfsWireError::InvalidWindowsDrive);
                };
                NativePathRoot::WindowsDrive {
                    drive: WindowsDrive::new(*drive)
                        .map_err(|_| VfsWireError::InvalidWindowsDrive)?,
                    form: if case.root_kind == "windows_verbatim_drive" {
                        WindowsDriveForm::VerbatimAbsolute
                    } else {
                        WindowsDriveForm::Absolute
                    },
                }
            }
            "windows_unc" => NativePathRoot::WindowsUnc {
                server: ExactComponent::new(
                    PathEncoding::WindowsWtf8,
                    take_first_component(&mut components)?,
                )?,
                share: ExactComponent::new(
                    PathEncoding::WindowsWtf8,
                    take_first_component(&mut components)?,
                )?,
                form: WindowsUncForm::Standard,
            },
            _ => return Err(VfsWireError::InvalidNativeRoot),
        };
        let provider_path = ProviderPath::from_byte_components(encoding, &components)?;
        let native_path = NativePath::new(root, provider_path)?;
        let wire = NativePathV2::from_native_path(&native_path);
        let decoded = protobuf_round_trip_native_path(&wire)?;
        let round_tripped = decoded.to_native_path()?;
        assert_eq!(round_tripped, native_path);
        Ok(())
    }

    fn protobuf_round_trip_provider_path(
        wire: &ProviderPathV2,
    ) -> Result<ProviderPathV2, VfsWireError> {
        ProviderPathV2::decode(wire.encode_to_vec().as_slice()).map_err(Into::into)
    }

    fn protobuf_round_trip_native_path(wire: &NativePathV2) -> Result<NativePathV2, VfsWireError> {
        NativePathV2::decode(wire.encode_to_vec().as_slice()).map_err(Into::into)
    }

    fn take_first_component(components: &mut Vec<Vec<u8>>) -> Result<Vec<u8>, VfsWireError> {
        if components.is_empty() {
            return Err(VfsWireError::MissingField("native root component"));
        }
        Ok(components.remove(0))
    }

    fn encoding_from_fixture(encoding: &str) -> Result<PathEncoding, VfsWireError> {
        match encoding {
            "unix_bytes" => Ok(PathEncoding::UnixBytes),
            "windows_wtf8" => Ok(PathEncoding::WindowsWtf8),
            "portable_utf8" => Ok(PathEncoding::PortableUtf8),
            _ => Err(VfsWireError::UnknownPathEncoding(-1)),
        }
    }

    fn decode_hex(hex: &str) -> Result<Vec<u8>, VfsWireError> {
        if !hex.len().is_multiple_of(2) {
            return Err(VfsWireError::InvalidNativeRoot);
        }
        let mut bytes = Vec::with_capacity(hex.len() / 2);
        for pair in hex.as_bytes().chunks_exact(2) {
            let high = hex_digit(pair[0])?;
            let low = hex_digit(pair[1])?;
            bytes.push((high << 4) | low);
        }
        Ok(bytes)
    }

    fn hex_digit(byte: u8) -> Result<u8, VfsWireError> {
        match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'a'..=b'f' => Ok(byte - b'a' + 10),
            b'A'..=b'F' => Ok(byte - b'A' + 10),
            _ => Err(VfsWireError::InvalidNativeRoot),
        }
    }
}
