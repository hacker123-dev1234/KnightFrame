use crate::error::{KfResult, LocalizedError};

const PREFIX: &str = "KnightFrame/provider/";

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
pub fn write(id: &str, secret: &str) -> KfResult<String> {
    use windows_sys::Win32::Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredWriteW,
    };
    let reference = format!("{PREFIX}{id}");
    let mut target = wide(&reference);
    let mut user = wide("KnightFrame");
    let mut blob = secret
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    let credential = CREDENTIALW {
        Flags: 0,
        Type: CRED_TYPE_GENERIC,
        TargetName: target.as_mut_ptr(),
        Comment: std::ptr::null_mut(),
        LastWritten: Default::default(),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        AttributeCount: 0,
        Attributes: std::ptr::null_mut(),
        TargetAlias: std::ptr::null_mut(),
        UserName: user.as_mut_ptr(),
    };
    if unsafe { CredWriteW(&credential, 0) } == 0 {
        return Err(LocalizedError::new("error.credential_write"));
    }
    blob.fill(0);
    Ok(reference)
}

#[cfg(windows)]
pub fn read(reference: &str) -> KfResult<String> {
    use windows_sys::Win32::Security::Credentials::{
        CRED_TYPE_GENERIC, CREDENTIALW, CredFree, CredReadW,
    };
    let target = wide(reference);
    let mut raw: *mut CREDENTIALW = std::ptr::null_mut();
    if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut raw) } == 0 || raw.is_null() {
        return Err(LocalizedError::new("error.credential_read"));
    }
    let credential = unsafe { &*raw };
    let bytes = unsafe {
        std::slice::from_raw_parts(
            credential.CredentialBlob,
            credential.CredentialBlobSize as usize,
        )
    };
    let units = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    let secret =
        String::from_utf16(&units).map_err(|_| LocalizedError::new("error.credential_read"));
    unsafe { CredFree(raw.cast()) };
    secret
}

#[cfg(windows)]
pub fn delete(reference: &str) {
    use windows_sys::Win32::Security::Credentials::{CRED_TYPE_GENERIC, CredDeleteW};
    let target = wide(reference);
    unsafe {
        CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0);
    }
}

#[cfg(not(windows))]
pub fn write(_: &str, _: &str) -> KfResult<String> {
    Err(LocalizedError::new("error.credential_unsupported"))
}
#[cfg(not(windows))]
pub fn read(_: &str) -> KfResult<String> {
    Err(LocalizedError::new("error.credential_unsupported"))
}
#[cfg(not(windows))]
pub fn delete(_: &str) {}
