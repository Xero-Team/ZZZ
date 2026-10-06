#![forbid(unsafe_code)]

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use std::{
    cmp::Ordering,
    ffi::OsString,
    fmt,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
};
use typed_path::{WindowsComponent, WindowsPath, WindowsPrefix};

mod manager;
mod memory_provider;
mod provider;
mod snapshot;

pub use manager::*;
pub use memory_provider::*;
pub use provider::*;
pub use snapshot::*;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PathEncoding {
    UnixBytes,
    WindowsWtf8,
    PortableUtf8,
}

impl fmt::Display for PathEncoding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnixBytes => formatter.write_str("unix bytes"),
            Self::WindowsWtf8 => formatter.write_str("Windows WTF-8"),
            Self::PortableUtf8 => formatter.write_str("portable UTF-8"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PathError {
    #[error("path components cannot be empty")]
    EmptyComponent,
    #[error("path components cannot contain NUL")]
    NullByte,
    #[error("path components cannot contain a separator for {encoding}")]
    Separator { encoding: PathEncoding },
    #[error("current-directory components are not normalized")]
    CurrentDirectory,
    #[error("parent-directory components are not normalized")]
    ParentDirectory,
    #[error("portable path component is not valid UTF-8")]
    InvalidPortableUtf8,
    #[error("invalid WTF-8 at byte {offset}")]
    InvalidWindowsWtf8 { offset: usize },
    #[error("Windows WTF-8 component contains a surrogate at byte {offset}")]
    WindowsSurrogate { offset: usize },
    #[error("path encoding mismatch: expected {expected}, found {actual}")]
    EncodingMismatch {
        expected: PathEncoding,
        actual: PathEncoding,
    },
    #[error("path is not under the requested prefix")]
    PrefixMismatch,
    #[error("path traversal would escape the root")]
    PathEscapesRoot,
    #[error("native root is incompatible with {encoding}")]
    RootEncodingMismatch { encoding: PathEncoding },
    #[error("Windows drive must be an ASCII letter")]
    InvalidWindowsDrive,
    #[error("absolute paths are not accepted by the legacy relative-path adapter")]
    AbsolutePathNotAllowed,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ExactComponent {
    encoding: PathEncoding,
    bytes: Arc<[u8]>,
}

impl ExactComponent {
    pub fn new(encoding: PathEncoding, bytes: impl Into<Arc<[u8]>>) -> Result<Self, PathError> {
        let bytes = bytes.into();
        validate_component(encoding, &bytes)?;
        Ok(Self { encoding, bytes })
    }

    pub fn encoding(&self) -> PathEncoding {
        self.encoding
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn display(&self) -> DisplayComponent {
        let display = match self.encoding {
            PathEncoding::UnixBytes => String::from_utf8_lossy(&self.bytes).into_owned(),
            PathEncoding::PortableUtf8 => String::from_utf8_lossy(&self.bytes).into_owned(),
            PathEncoding::WindowsWtf8 => match wtf8_to_string(&self.bytes, false) {
                Ok(display) => display,
                Err(_) => String::from("\u{fffd}"),
            },
        };
        DisplayComponent(display.into())
    }

    pub fn default_lookup_key(&self) -> LookupKey {
        LookupKey(self.bytes.clone())
    }

    pub fn to_windows_wide(&self) -> Result<Vec<u16>, PathError> {
        if self.encoding != PathEncoding::WindowsWtf8 {
            return Err(PathError::EncodingMismatch {
                expected: PathEncoding::WindowsWtf8,
                actual: self.encoding,
            });
        }
        decode_wtf8_to_wide(&self.bytes)
    }
}

#[derive(Deserialize)]
struct ExactComponentRepresentation {
    encoding: PathEncoding,
    bytes: Vec<u8>,
}

impl<'de> Deserialize<'de> for ExactComponent {
    fn deserialize<DeserializerType>(
        deserializer: DeserializerType,
    ) -> Result<Self, DeserializerType::Error>
    where
        DeserializerType: Deserializer<'de>,
    {
        let representation = ExactComponentRepresentation::deserialize(deserializer)?;
        Self::new(representation.encoding, representation.bytes)
            .map_err(DeserializerType::Error::custom)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct DisplayComponent(Arc<str>);

impl DisplayComponent {
    pub fn new(display: impl Into<Arc<str>>) -> Self {
        Self(display.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DisplayComponent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct LookupKey(Arc<[u8]>);

impl LookupKey {
    pub fn new(bytes: impl Into<Arc<[u8]>>) -> Self {
        Self(bytes.into())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct EntryName {
    exact: ExactComponent,
    display: DisplayComponent,
    lookup_key: LookupKey,
}

impl EntryName {
    pub fn new(exact: ExactComponent, display: DisplayComponent, lookup_key: LookupKey) -> Self {
        Self {
            exact,
            display,
            lookup_key,
        }
    }

    pub fn from_exact(exact: ExactComponent) -> Self {
        let display = exact.display();
        let lookup_key = exact.default_lookup_key();
        Self::new(exact, display, lookup_key)
    }

    pub fn exact(&self) -> &ExactComponent {
        &self.exact
    }

    pub fn display(&self) -> &DisplayComponent {
        &self.display
    }

    pub fn lookup_key(&self) -> &LookupKey {
        &self.lookup_key
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ProviderPath {
    encoding: PathEncoding,
    components: Arc<[ExactComponent]>,
}

impl ProviderPath {
    pub fn root(encoding: PathEncoding) -> Self {
        Self {
            encoding,
            components: Arc::from([]),
        }
    }

    pub fn from_exact_components(
        encoding: PathEncoding,
        components: impl IntoIterator<Item = ExactComponent>,
    ) -> Result<Self, PathError> {
        let components = components.into_iter().collect::<Vec<_>>();
        for component in &components {
            if component.encoding != encoding {
                return Err(PathError::EncodingMismatch {
                    expected: encoding,
                    actual: component.encoding,
                });
            }
        }
        Ok(Self {
            encoding,
            components: components.into(),
        })
    }

    pub fn from_byte_components<Components, ComponentBytes>(
        encoding: PathEncoding,
        components: Components,
    ) -> Result<Self, PathError>
    where
        Components: IntoIterator<Item = ComponentBytes>,
        ComponentBytes: AsRef<[u8]>,
    {
        let components = components
            .into_iter()
            .map(|component| ExactComponent::new(encoding, component.as_ref().to_vec()))
            .collect::<Result<Vec<_>, _>>()?;
        Self::from_exact_components(encoding, components)
    }

    pub fn encoding(&self) -> PathEncoding {
        self.encoding
    }

    pub fn components(&self) -> impl ExactSizeIterator<Item = &ExactComponent> {
        self.components.iter()
    }

    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    pub fn is_root(&self) -> bool {
        self.components.is_empty()
    }

    pub fn file_name(&self) -> Option<&ExactComponent> {
        self.components.last()
    }

    pub fn parent(&self) -> Option<Self> {
        let parent_components = self
            .components
            .get(..self.components.len().checked_sub(1)?)?;
        Some(Self {
            encoding: self.encoding,
            components: Arc::from(parent_components),
        })
    }

    pub fn join_component(&self, component: ExactComponent) -> Result<Self, PathError> {
        if component.encoding != self.encoding {
            return Err(PathError::EncodingMismatch {
                expected: self.encoding,
                actual: component.encoding,
            });
        }
        let mut components = Vec::with_capacity(self.components.len() + 1);
        components.extend(self.components.iter().cloned());
        components.push(component);
        Self::from_exact_components(self.encoding, components)
    }

    pub fn join(&self, suffix: &Self) -> Result<Self, PathError> {
        if suffix.encoding != self.encoding {
            return Err(PathError::EncodingMismatch {
                expected: self.encoding,
                actual: suffix.encoding,
            });
        }
        let mut components = Vec::with_capacity(self.components.len() + suffix.components.len());
        components.extend(self.components.iter().cloned());
        components.extend(suffix.components.iter().cloned());
        Self::from_exact_components(self.encoding, components)
    }

    pub fn starts_with(&self, prefix: &Self) -> bool {
        self.encoding == prefix.encoding && self.components.starts_with(&prefix.components)
    }

    pub fn strip_prefix(&self, prefix: &Self) -> Result<Self, PathError> {
        if self.encoding != prefix.encoding {
            return Err(PathError::EncodingMismatch {
                expected: self.encoding,
                actual: prefix.encoding,
            });
        }
        let Some(components) = self.components.strip_prefix(prefix.components.as_ref()) else {
            return Err(PathError::PrefixMismatch);
        };
        Ok(Self {
            encoding: self.encoding,
            components: Arc::from(components),
        })
    }

    pub fn display(&self) -> String {
        let separator = match self.encoding {
            PathEncoding::WindowsWtf8 => "\\",
            PathEncoding::UnixBytes | PathEncoding::PortableUtf8 => "/",
        };
        let mut display = String::new();
        for (component_index, component) in self.components.iter().enumerate() {
            if component_index > 0 {
                display.push_str(separator);
            }
            display.push_str(component.display().as_str());
        }
        display
    }
}

#[derive(Deserialize)]
struct ProviderPathRepresentation {
    encoding: PathEncoding,
    components: Vec<ExactComponent>,
}

impl<'de> Deserialize<'de> for ProviderPath {
    fn deserialize<DeserializerType>(
        deserializer: DeserializerType,
    ) -> Result<Self, DeserializerType::Error>
    where
        DeserializerType: Deserializer<'de>,
    {
        let representation = ProviderPathRepresentation::deserialize(deserializer)?;
        Self::from_exact_components(representation.encoding, representation.components)
            .map_err(DeserializerType::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct MountId(u64);

impl MountId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ResourceId {
    mount_id: MountId,
    node_id: u64,
    generation: u32,
}

impl ResourceId {
    pub const fn new(mount_id: MountId, node_id: u64, generation: u32) -> Self {
        Self {
            mount_id,
            node_id,
            generation,
        }
    }

    pub const fn mount_id(self) -> MountId {
        self.mount_id
    }

    pub const fn node_id(self) -> u64 {
        self.node_id
    }

    pub const fn generation(self) -> u32 {
        self.generation
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct VfsPath {
    mount_id: MountId,
    path: ProviderPath,
}

impl VfsPath {
    pub fn new(mount_id: MountId, path: ProviderPath) -> Self {
        Self { mount_id, path }
    }

    pub fn mount_id(&self) -> MountId {
        self.mount_id
    }

    pub fn provider_path(&self) -> &ProviderPath {
        &self.path
    }

    pub fn join_component(&self, component: ExactComponent) -> Result<Self, PathError> {
        Ok(Self::new(
            self.mount_id,
            self.path.join_component(component)?,
        ))
    }

    pub fn parent(&self) -> Option<Self> {
        Some(Self::new(self.mount_id, self.path.parent()?))
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(transparent)]
pub struct WindowsDrive(u8);

impl WindowsDrive {
    pub fn new(drive: u8) -> Result<Self, PathError> {
        if !drive.is_ascii_alphabetic() {
            return Err(PathError::InvalidWindowsDrive);
        }
        Ok(Self(drive))
    }

    pub const fn as_ascii(self) -> u8 {
        self.0
    }
}

impl PartialEq for WindowsDrive {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }
}

impl Eq for WindowsDrive {}

impl Hash for WindowsDrive {
    fn hash<HasherType: Hasher>(&self, state: &mut HasherType) {
        self.0.to_ascii_uppercase().hash(state);
    }
}

impl PartialOrd for WindowsDrive {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WindowsDrive {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0
            .to_ascii_uppercase()
            .cmp(&other.0.to_ascii_uppercase())
    }
}

impl<'de> Deserialize<'de> for WindowsDrive {
    fn deserialize<DeserializerType>(
        deserializer: DeserializerType,
    ) -> Result<Self, DeserializerType::Error>
    where
        DeserializerType: Deserializer<'de>,
    {
        Self::new(u8::deserialize(deserializer)?).map_err(DeserializerType::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowsDriveForm {
    Relative,
    Absolute,
    VerbatimRelative,
    VerbatimAbsolute,
}

impl WindowsDriveForm {
    pub const fn is_absolute(self) -> bool {
        matches!(self, Self::Absolute | Self::VerbatimAbsolute)
    }

    pub const fn is_verbatim(self) -> bool {
        matches!(self, Self::VerbatimRelative | Self::VerbatimAbsolute)
    }

    fn make_absolute(&mut self) {
        *self = match self {
            Self::Relative | Self::Absolute => Self::Absolute,
            Self::VerbatimRelative | Self::VerbatimAbsolute => Self::VerbatimAbsolute,
        };
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowsUncForm {
    Standard,
    Verbatim,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NativePathRoot {
    Relative,
    Posix,
    WindowsRootRelative,
    WindowsDrive {
        drive: WindowsDrive,
        form: WindowsDriveForm,
    },
    WindowsUnc {
        server: ExactComponent,
        share: ExactComponent,
        form: WindowsUncForm,
    },
    WindowsDevice {
        device: ExactComponent,
    },
    WindowsVerbatim {
        namespace: ExactComponent,
    },
}

impl NativePathRoot {
    pub fn encoding(&self) -> Option<PathEncoding> {
        match self {
            Self::Relative => None,
            Self::Posix => Some(PathEncoding::UnixBytes),
            Self::WindowsRootRelative
            | Self::WindowsDrive { .. }
            | Self::WindowsUnc { .. }
            | Self::WindowsDevice { .. }
            | Self::WindowsVerbatim { .. } => Some(PathEncoding::WindowsWtf8),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct NativePath {
    root: NativePathRoot,
    path: ProviderPath,
}

impl NativePath {
    pub fn new(root: NativePathRoot, path: ProviderPath) -> Result<Self, PathError> {
        if let Some(root_encoding) = root.encoding()
            && root_encoding != path.encoding
        {
            return Err(PathError::RootEncodingMismatch {
                encoding: path.encoding,
            });
        }
        validate_native_root(&root)?;
        Ok(Self { root, path })
    }

    pub fn root(&self) -> &NativePathRoot {
        &self.root
    }

    pub fn provider_path(&self) -> &ProviderPath {
        &self.path
    }

    pub fn from_unix_bytes(bytes: &[u8]) -> Result<Self, PathError> {
        let (root, relative_bytes) = match bytes.strip_prefix(b"/") {
            Some(relative_bytes) => (NativePathRoot::Posix, relative_bytes),
            None => (NativePathRoot::Relative, bytes),
        };
        let path = normalized_path_from_separated_bytes(
            PathEncoding::UnixBytes,
            relative_bytes,
            |byte| byte == b'/',
        )?;
        Self::new(root, path)
    }

    pub fn to_unix_bytes(&self) -> Result<Vec<u8>, PathError> {
        if self.path.encoding != PathEncoding::UnixBytes {
            return Err(PathError::EncodingMismatch {
                expected: PathEncoding::UnixBytes,
                actual: self.path.encoding,
            });
        }
        let mut bytes = Vec::new();
        match self.root {
            NativePathRoot::Relative => {}
            NativePathRoot::Posix => bytes.push(b'/'),
            _ => {
                return Err(PathError::RootEncodingMismatch {
                    encoding: self.path.encoding,
                });
            }
        }
        for (component_index, component) in self.path.components.iter().enumerate() {
            if component_index > 0 || matches!(self.root, NativePathRoot::Posix) {
                if bytes.last().copied() != Some(b'/') {
                    bytes.push(b'/');
                }
            }
            bytes.extend_from_slice(component.as_bytes());
        }
        Ok(bytes)
    }

    pub fn from_windows_wide(wide: &[u16]) -> Result<Self, PathError> {
        let encoded = encode_windows_wide(wide);
        let mut root = NativePathRoot::Relative;
        let mut exact_components = Vec::new();

        for component in WindowsPath::new(&encoded).components() {
            match component {
                WindowsComponent::Prefix(prefix) => {
                    root = match prefix.kind() {
                        WindowsPrefix::Verbatim(namespace) => NativePathRoot::WindowsVerbatim {
                            namespace: ExactComponent::new(
                                PathEncoding::WindowsWtf8,
                                namespace.to_vec(),
                            )?,
                        },
                        WindowsPrefix::VerbatimUNC(server, share) => NativePathRoot::WindowsUnc {
                            server: ExactComponent::new(
                                PathEncoding::WindowsWtf8,
                                server.to_vec(),
                            )?,
                            share: ExactComponent::new(PathEncoding::WindowsWtf8, share.to_vec())?,
                            form: WindowsUncForm::Verbatim,
                        },
                        WindowsPrefix::VerbatimDisk(_) => NativePathRoot::WindowsDrive {
                            drive: windows_drive_from_prefix(prefix.as_bytes())?,
                            form: WindowsDriveForm::VerbatimRelative,
                        },
                        WindowsPrefix::DeviceNS(device) => NativePathRoot::WindowsDevice {
                            device: ExactComponent::new(
                                PathEncoding::WindowsWtf8,
                                device.to_vec(),
                            )?,
                        },
                        WindowsPrefix::UNC(server, share) => NativePathRoot::WindowsUnc {
                            server: ExactComponent::new(
                                PathEncoding::WindowsWtf8,
                                server.to_vec(),
                            )?,
                            share: ExactComponent::new(PathEncoding::WindowsWtf8, share.to_vec())?,
                            form: WindowsUncForm::Standard,
                        },
                        WindowsPrefix::Disk(_) => NativePathRoot::WindowsDrive {
                            drive: windows_drive_from_prefix(prefix.as_bytes())?,
                            form: WindowsDriveForm::Relative,
                        },
                    };
                }
                WindowsComponent::RootDir => match &mut root {
                    NativePathRoot::Relative => root = NativePathRoot::WindowsRootRelative,
                    NativePathRoot::WindowsDrive { form, .. } => form.make_absolute(),
                    _ => {}
                },
                WindowsComponent::CurDir => {}
                WindowsComponent::ParentDir => {
                    if exact_components.pop().is_none() {
                        return Err(PathError::PathEscapesRoot);
                    }
                }
                WindowsComponent::Normal(component) => exact_components.push(ExactComponent::new(
                    PathEncoding::WindowsWtf8,
                    component.to_vec(),
                )?),
            }
        }

        let path =
            ProviderPath::from_exact_components(PathEncoding::WindowsWtf8, exact_components)?;
        Self::new(root, path)
    }

    pub fn to_windows_wide(&self) -> Result<Vec<u16>, PathError> {
        if self.path.encoding != PathEncoding::WindowsWtf8 {
            return Err(PathError::EncodingMismatch {
                expected: PathEncoding::WindowsWtf8,
                actual: self.path.encoding,
            });
        }

        let mut wide = Vec::new();
        let mut first_component_without_separator = false;
        match &self.root {
            NativePathRoot::Relative => first_component_without_separator = true,
            NativePathRoot::WindowsRootRelative => wide.push(u16::from(b'\\')),
            NativePathRoot::WindowsDrive { drive, form } => {
                if form.is_verbatim() {
                    push_ascii_wide(&mut wide, br"\\?\");
                }
                wide.push(u16::from(drive.as_ascii()));
                wide.push(u16::from(b':'));
                if form.is_absolute() {
                    wide.push(u16::from(b'\\'));
                } else {
                    first_component_without_separator = true;
                }
            }
            NativePathRoot::WindowsUnc {
                server,
                share,
                form,
            } => {
                if *form == WindowsUncForm::Verbatim {
                    push_ascii_wide(&mut wide, br"\\?\UNC\");
                } else {
                    push_ascii_wide(&mut wide, br"\\");
                }
                wide.extend(server.to_windows_wide()?);
                wide.push(u16::from(b'\\'));
                wide.extend(share.to_windows_wide()?);
            }
            NativePathRoot::WindowsDevice { device } => {
                push_ascii_wide(&mut wide, br"\\.\");
                wide.extend(device.to_windows_wide()?);
            }
            NativePathRoot::WindowsVerbatim { namespace } => {
                push_ascii_wide(&mut wide, br"\\?\");
                wide.extend(namespace.to_windows_wide()?);
            }
            NativePathRoot::Posix => {
                return Err(PathError::RootEncodingMismatch {
                    encoding: self.path.encoding,
                });
            }
        }

        for (component_index, component) in self.path.components.iter().enumerate() {
            if !(component_index == 0 && first_component_without_separator)
                && wide.last().copied() != Some(u16::from(b'\\'))
            {
                wide.push(u16::from(b'\\'));
            }
            wide.extend(component.to_windows_wide()?);
        }
        Ok(wide)
    }

    pub fn from_local_path(path: &Path) -> Result<Self, PathError> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt as _;
            Self::from_unix_bytes(path.as_os_str().as_bytes())
        }

        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt as _;
            Self::from_windows_wide(&path.as_os_str().encode_wide().collect::<Vec<_>>())
        }
    }

    pub fn to_local_path_buf(&self) -> Result<PathBuf, PathError> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt as _;
            Ok(PathBuf::from(OsString::from_vec(self.to_unix_bytes()?)))
        }

        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt as _;
            Ok(PathBuf::from(OsString::from_wide(&self.to_windows_wide()?)))
        }
    }
}

#[derive(Deserialize)]
struct NativePathRepresentation {
    root: NativePathRoot,
    path: ProviderPath,
}

impl<'de> Deserialize<'de> for NativePath {
    fn deserialize<DeserializerType>(
        deserializer: DeserializerType,
    ) -> Result<Self, DeserializerType::Error>
    where
        DeserializerType: Deserializer<'de>,
    {
        let representation = NativePathRepresentation::deserialize(deserializer)?;
        Self::new(representation.root, representation.path).map_err(DeserializerType::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CompatibilityPathError {
    #[error(transparent)]
    InvalidPath(#[from] PathError),
    #[error("component {component_index} encoded as {encoding} cannot be represented as UTF-8")]
    UnrepresentableComponent {
        component_index: usize,
        encoding: PathEncoding,
    },
}

pub fn provider_path_from_legacy_utf8(
    path: &str,
    encoding: PathEncoding,
) -> Result<ProviderPath, CompatibilityPathError> {
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(PathError::AbsolutePathNotAllowed.into());
    }
    if path.is_empty() {
        return Ok(ProviderPath::root(encoding));
    }
    ProviderPath::from_byte_components(encoding, path.split('/').map(str::as_bytes))
        .map_err(Into::into)
}

pub fn provider_path_to_legacy_utf8(path: &ProviderPath) -> Result<String, CompatibilityPathError> {
    let mut result = String::new();
    for (component_index, component) in path.components.iter().enumerate() {
        if component_index > 0 {
            result.push('/');
        }
        let component_string = match component.encoding {
            PathEncoding::UnixBytes | PathEncoding::PortableUtf8 => {
                std::str::from_utf8(component.as_bytes())
                    .ok()
                    .map(str::to_owned)
            }
            PathEncoding::WindowsWtf8 => wtf8_to_string(component.as_bytes(), true).ok(),
        };
        let Some(component_string) = component_string else {
            return Err(CompatibilityPathError::UnrepresentableComponent {
                component_index,
                encoding: component.encoding,
            });
        };
        result.push_str(&component_string);
    }
    Ok(result)
}

fn validate_component(encoding: PathEncoding, bytes: &[u8]) -> Result<(), PathError> {
    if bytes.is_empty() {
        return Err(PathError::EmptyComponent);
    }
    if bytes.contains(&0) {
        return Err(PathError::NullByte);
    }
    if bytes == b"." {
        return Err(PathError::CurrentDirectory);
    }
    if bytes == b".." {
        return Err(PathError::ParentDirectory);
    }
    let has_separator = match encoding {
        PathEncoding::UnixBytes | PathEncoding::PortableUtf8 => bytes.contains(&b'/'),
        PathEncoding::WindowsWtf8 => bytes.contains(&b'/') || bytes.contains(&b'\\'),
    };
    if has_separator {
        return Err(PathError::Separator { encoding });
    }
    match encoding {
        PathEncoding::UnixBytes => Ok(()),
        PathEncoding::PortableUtf8 => std::str::from_utf8(bytes)
            .map(|_| ())
            .map_err(|_| PathError::InvalidPortableUtf8),
        PathEncoding::WindowsWtf8 => visit_wtf8_code_points(bytes, |_, _| Ok(())),
    }
}

fn validate_native_root(root: &NativePathRoot) -> Result<(), PathError> {
    let windows_components = match root {
        NativePathRoot::WindowsUnc { server, share, .. } => [Some(server), Some(share)],
        NativePathRoot::WindowsDevice { device } => [Some(device), None],
        NativePathRoot::WindowsVerbatim { namespace } => [Some(namespace), None],
        _ => [None, None],
    };
    for component in windows_components.into_iter().flatten() {
        if component.encoding != PathEncoding::WindowsWtf8 {
            return Err(PathError::RootEncodingMismatch {
                encoding: component.encoding,
            });
        }
    }
    Ok(())
}

fn normalized_path_from_separated_bytes(
    encoding: PathEncoding,
    bytes: &[u8],
    is_separator: impl Fn(u8) -> bool,
) -> Result<ProviderPath, PathError> {
    let mut components = Vec::new();
    for component in bytes.split(|byte| is_separator(*byte)) {
        if component.is_empty() || component == b"." {
            continue;
        }
        if component == b".." {
            if components.pop().is_none() {
                return Err(PathError::PathEscapesRoot);
            }
            continue;
        }
        components.push(ExactComponent::new(encoding, component.to_vec())?);
    }
    ProviderPath::from_exact_components(encoding, components)
}

fn encode_windows_wide(wide: &[u16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(wide.len());
    let mut index = 0;
    while let Some(code_unit) = wide.get(index).copied() {
        if (0xd800..=0xdbff).contains(&code_unit)
            && let Some(low_surrogate) = wide.get(index + 1).copied()
            && (0xdc00..=0xdfff).contains(&low_surrogate)
        {
            let code_point = 0x10000
                + ((u32::from(code_unit) - 0xd800) << 10)
                + (u32::from(low_surrogate) - 0xdc00);
            encode_code_point(code_point, &mut bytes);
            index += 2;
            continue;
        }
        encode_code_point(u32::from(code_unit), &mut bytes);
        index += 1;
    }
    bytes
}

fn encode_code_point(code_point: u32, bytes: &mut Vec<u8>) {
    if code_point <= 0x7f {
        bytes.push(code_point as u8);
    } else if code_point <= 0x7ff {
        bytes.push((0xc0 | (code_point >> 6)) as u8);
        bytes.push((0x80 | (code_point & 0x3f)) as u8);
    } else if code_point <= 0xffff {
        bytes.push((0xe0 | (code_point >> 12)) as u8);
        bytes.push((0x80 | ((code_point >> 6) & 0x3f)) as u8);
        bytes.push((0x80 | (code_point & 0x3f)) as u8);
    } else {
        bytes.push((0xf0 | (code_point >> 18)) as u8);
        bytes.push((0x80 | ((code_point >> 12) & 0x3f)) as u8);
        bytes.push((0x80 | ((code_point >> 6) & 0x3f)) as u8);
        bytes.push((0x80 | (code_point & 0x3f)) as u8);
    }
}

fn decode_wtf8_to_wide(bytes: &[u8]) -> Result<Vec<u16>, PathError> {
    let mut wide = Vec::new();
    visit_wtf8_code_points(bytes, |_, code_point| {
        if code_point <= 0xffff {
            wide.push(code_point as u16);
        } else {
            let scalar = code_point - 0x10000;
            wide.push((0xd800 + (scalar >> 10)) as u16);
            wide.push((0xdc00 + (scalar & 0x3ff)) as u16);
        }
        Ok(())
    })?;
    Ok(wide)
}

fn wtf8_to_string(bytes: &[u8], reject_surrogates: bool) -> Result<String, PathError> {
    let mut string = String::new();
    visit_wtf8_code_points(bytes, |offset, code_point| {
        if (0xd800..=0xdfff).contains(&code_point) {
            if reject_surrogates {
                return Err(PathError::WindowsSurrogate { offset });
            }
            string.push('\u{fffd}');
        } else if let Some(character) = char::from_u32(code_point) {
            string.push(character);
        }
        Ok(())
    })?;
    Ok(string)
}

fn visit_wtf8_code_points(
    bytes: &[u8],
    mut visitor: impl FnMut(usize, u32) -> Result<(), PathError>,
) -> Result<(), PathError> {
    let mut offset = 0;
    while let Some(first) = bytes.get(offset).copied() {
        let (code_point, length) = match first {
            0x00..=0x7f => (u32::from(first), 1),
            0xc2..=0xdf => {
                let second = continuation_byte(bytes, offset, 1)?;
                ((u32::from(first & 0x1f) << 6) | u32::from(second & 0x3f), 2)
            }
            0xe0..=0xef => {
                let second = continuation_byte(bytes, offset, 1)?;
                let third = continuation_byte(bytes, offset, 2)?;
                if first == 0xe0 && second < 0xa0 {
                    return Err(PathError::InvalidWindowsWtf8 { offset });
                }
                let code_point = (u32::from(first & 0x0f) << 12)
                    | (u32::from(second & 0x3f) << 6)
                    | u32::from(third & 0x3f);
                (code_point, 3)
            }
            0xf0..=0xf4 => {
                let second = continuation_byte(bytes, offset, 1)?;
                let third = continuation_byte(bytes, offset, 2)?;
                let fourth = continuation_byte(bytes, offset, 3)?;
                if (first == 0xf0 && second < 0x90) || (first == 0xf4 && second > 0x8f) {
                    return Err(PathError::InvalidWindowsWtf8 { offset });
                }
                let code_point = (u32::from(first & 0x07) << 18)
                    | (u32::from(second & 0x3f) << 12)
                    | (u32::from(third & 0x3f) << 6)
                    | u32::from(fourth & 0x3f);
                (code_point, 4)
            }
            _ => return Err(PathError::InvalidWindowsWtf8 { offset }),
        };
        visitor(offset, code_point)?;
        offset += length;
    }
    Ok(())
}

fn continuation_byte(bytes: &[u8], offset: usize, relative: usize) -> Result<u8, PathError> {
    let Some(byte) = bytes.get(offset + relative).copied() else {
        return Err(PathError::InvalidWindowsWtf8 { offset });
    };
    if byte & 0xc0 != 0x80 {
        return Err(PathError::InvalidWindowsWtf8 { offset });
    }
    Ok(byte)
}

fn push_ascii_wide(wide: &mut Vec<u16>, bytes: &[u8]) {
    wide.extend(bytes.iter().copied().map(u16::from));
}

fn windows_drive_from_prefix(prefix: &[u8]) -> Result<WindowsDrive, PathError> {
    for drive_and_colon in prefix.windows(2) {
        if let [drive, b':'] = drive_and_colon
            && drive.is_ascii_alphabetic()
        {
            return WindowsDrive::new(*drive);
        }
    }
    Err(PathError::InvalidWindowsDrive)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn portable_component(bytes: &[u8]) -> ExactComponent {
        let result = ExactComponent::new(PathEncoding::PortableUtf8, bytes.to_vec());
        let Ok(component) = result else {
            panic!("portable test component must be valid: {result:?}");
        };
        component
    }

    #[test]
    fn relative_path_operations_preserve_exact_identity() {
        let result = ProviderPath::from_exact_components(
            PathEncoding::PortableUtf8,
            [portable_component(b"src"), portable_component(b"main.rs")],
        );
        let Ok(path) = result else {
            panic!("test path must be valid: {result:?}");
        };
        let root = ProviderPath::root(PathEncoding::PortableUtf8);
        let Some(parent) = path.parent() else {
            panic!("two-component path must have a parent");
        };
        assert_eq!(parent.display(), "src");
        assert_eq!(
            path.strip_prefix(&parent).map(|path| path.display()),
            Ok(String::from("main.rs"))
        );
        assert_eq!(path.strip_prefix(&root), Ok(path.clone()));
        assert!(path.starts_with(&root));
        assert_eq!(
            path.file_name().map(ExactComponent::as_bytes),
            Some(b"main.rs".as_slice())
        );
    }

    #[test]
    fn components_reject_ambiguous_or_lossy_identity() {
        for bytes in [
            b"".as_slice(),
            b".".as_slice(),
            b"..".as_slice(),
            b"a/b".as_slice(),
            b"a\0b".as_slice(),
        ] {
            assert!(ExactComponent::new(PathEncoding::UnixBytes, bytes.to_vec()).is_err());
        }
        assert!(ExactComponent::new(PathEncoding::PortableUtf8, vec![0xff]).is_err());
        assert!(ExactComponent::new(PathEncoding::WindowsWtf8, vec![0xff]).is_err());
        assert!(ExactComponent::new(PathEncoding::WindowsWtf8, b"a\\b".to_vec()).is_err());
    }

    #[test]
    fn unix_native_paths_round_trip_non_utf8_bytes() {
        let input = b"/tmp/non-\x80-utf8";
        let parsed = NativePath::from_unix_bytes(input);
        let Ok(parsed) = parsed else {
            panic!("Unix byte path must parse: {parsed:?}");
        };
        assert_eq!(parsed.to_unix_bytes(), Ok(input.to_vec()));
        assert_eq!(parsed.provider_path().display(), "tmp/non-�-utf8");
    }

    #[test]
    fn windows_native_paths_round_trip_wtf16_and_roots() {
        let mut drive_path = "C:\\Users\\ZZZ\\".encode_utf16().collect::<Vec<_>>();
        drive_path.push(0xd800);
        drive_path.extend(".txt".encode_utf16());
        let parsed = NativePath::from_windows_wide(&drive_path);
        let Ok(parsed) = parsed else {
            panic!("Windows drive path must parse: {parsed:?}");
        };
        assert_eq!(parsed.to_windows_wide(), Ok(drive_path));

        for path in [
            r"\\server\share\folder\file.txt",
            r"\\?\C:\folder\file.txt",
            r"\\?\UNC\server\share\folder\file.txt",
            r"\\.\COM42\child",
            r"c:\lower\file.txt",
        ] {
            let wide = path.encode_utf16().collect::<Vec<_>>();
            let parsed = NativePath::from_windows_wide(&wide);
            let Ok(parsed) = parsed else {
                panic!("Windows root fixture must parse: {parsed:?}");
            };
            assert_eq!(parsed.to_windows_wide(), Ok(wide));
        }
    }

    #[test]
    fn legacy_adapter_rejects_non_unicode_components() {
        let path = ProviderPath::from_byte_components(
            PathEncoding::UnixBytes,
            [b"valid".as_slice(), b"non-\x80-utf8".as_slice()],
        );
        let Ok(path) = path else {
            panic!("Unix byte path must be valid: {path:?}");
        };
        assert!(matches!(
            provider_path_to_legacy_utf8(&path),
            Err(CompatibilityPathError::UnrepresentableComponent {
                component_index: 1,
                encoding: PathEncoding::UnixBytes,
            })
        ));
    }

    #[test]
    fn serde_round_trip_revalidates_paths() {
        let path_result = provider_path_from_legacy_utf8("src/main.rs", PathEncoding::PortableUtf8);
        let Ok(provider_path) = path_result else {
            panic!("test path must be valid: {path_result:?}");
        };
        let path = VfsPath::new(MountId::new(7), provider_path);
        let serialized = serde_json::to_vec(&path);
        let Ok(serialized) = serialized else {
            panic!("VFS path must serialize: {serialized:?}");
        };
        let deserialized = serde_json::from_slice::<VfsPath>(&serialized);
        let Ok(deserialized) = deserialized else {
            panic!("VFS path must deserialize: {deserialized:?}");
        };
        assert_eq!(deserialized, path);
    }
}
