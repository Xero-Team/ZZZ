use std::{
    io::{Error, ErrorKind, Result},
    path::Path,
    sync::Once,
};

use windows::Win32::Networking::WinSock::{
    ADDRESS_FAMILY, AF_UNIX, SOCKADDR_UN, SOCKET_ERROR, WSAGetLastError, WSAStartup,
};

pub(crate) fn init() {
    static ONCE: Once = Once::new();

    ONCE.call_once(|| unsafe {
        let mut wsa_data = std::mem::zeroed();
        let result = WSAStartup(0x202, &mut wsa_data);
        if result != 0 {
            panic!("WSAStartup failed: {}", result);
        }
    });
}

// https://devblogs.microsoft.com/commandline/af_unix-comes-to-windows/
pub(crate) fn sockaddr_un<P: AsRef<Path>>(path: P) -> Result<(SOCKADDR_UN, usize)> {
    let mut addr = SOCKADDR_UN::default();
    addr.sun_family = ADDRESS_FAMILY(AF_UNIX);

    let bytes = path
        .as_ref()
        .to_str()
        .map(|s| s.as_bytes())
        .ok_or(ErrorKind::InvalidInput)?;

    if bytes.contains(&0) {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "paths may not contain interior null bytes",
        ));
    }
    if bytes.len() >= addr.sun_path.len() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "path must be shorter than SUN_LEN",
        ));
    }

    unsafe {
        std::ptr::copy_nonoverlapping(
            bytes.as_ptr(),
            addr.sun_path.as_mut_ptr().cast(),
            bytes.len(),
        );
    }

    let mut len = sun_path_offset(&addr) + bytes.len();
    match bytes.first() {
        Some(&0) | None => {}
        Some(_) => len += 1,
    }
    Ok((addr, len))
}

pub(crate) fn map_ret(ret: i32) -> Result<usize> {
    if ret == SOCKET_ERROR {
        Err(Error::from_raw_os_error(unsafe { WSAGetLastError().0 }))
    } else {
        Ok(ret as usize)
    }
}

fn sun_path_offset(addr: &SOCKADDR_UN) -> usize {
    // Work with an actual instance of the type since using a null pointer is UB
    let base = addr as *const _ as usize;
    let path = &addr.sun_path as *const _ as usize;
    path - base
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sockaddr_un_encodes_path_and_length() {
        let path = Path::new("socket.sock");
        let (addr, len) = sockaddr_un(path).unwrap();
        let path_bytes = path.to_str().unwrap().as_bytes();

        assert_eq!(addr.sun_family, ADDRESS_FAMILY(AF_UNIX));
        assert_eq!(len, sun_path_offset(&addr) + path_bytes.len() + 1);

        let encoded_path: Vec<u8> = addr.sun_path[..path_bytes.len()]
            .iter()
            .map(|byte| *byte as u8)
            .collect();
        assert_eq!(encoded_path, path_bytes);
        assert_eq!(addr.sun_path[path_bytes.len()], 0);
    }

    #[test]
    fn sockaddr_un_rejects_interior_null_bytes() {
        let error = sockaddr_un(Path::new("socket\0.sock")).unwrap_err();

        assert_eq!(error.kind(), ErrorKind::InvalidInput);
        assert_eq!(
            error.to_string(),
            "paths may not contain interior null bytes"
        );
    }

    #[test]
    fn sockaddr_un_rejects_too_long_path() {
        let max_len = SOCKADDR_UN::default().sun_path.len();
        let path = "a".repeat(max_len);

        let error = sockaddr_un(Path::new(&path)).unwrap_err();

        assert_eq!(error.kind(), ErrorKind::InvalidInput);
        assert_eq!(error.to_string(), "path must be shorter than SUN_LEN");
    }
}
