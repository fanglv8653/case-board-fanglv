use zeroize::Zeroize;

use super::{CredentialError, CredentialLocator, SecretValue, MAX_SECRET_BYTES};

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

pub(super) fn set(
    locator: &CredentialLocator,
    secret: &SecretValue,
) -> Result<(), CredentialError> {
    use windows::core::PWSTR;
    use windows::Win32::Security::Credentials::{
        CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    if secret.expose().len() > MAX_SECRET_BYTES {
        return Err(CredentialError::InvalidSecret);
    }
    let mut target = wide_null(&locator.target_name());
    let mut username = wide_null("CaseBoard");
    let mut blob = secret.expose().as_bytes().to_vec();
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR(username.as_mut_ptr()),
        ..Default::default()
    };
    let result = unsafe { CredWriteW(&credential, 0) }.map_err(|_| CredentialError::SecureStore);
    blob.zeroize();
    result
}

pub(super) fn get(locator: &CredentialLocator) -> Result<Option<SecretValue>, CredentialError> {
    use std::ptr::null_mut;
    use windows::core::{HRESULT, PCWSTR};
    use windows::Win32::Foundation::ERROR_NOT_FOUND;
    use windows::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };

    let target = wide_null(&locator.target_name());
    let mut raw: *mut CREDENTIALW = null_mut();
    if let Err(error) =
        unsafe { CredReadW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None, &mut raw) }
    {
        if error.code() == HRESULT::from_win32(ERROR_NOT_FOUND.0) {
            return Ok(None);
        }
        return Err(CredentialError::SecureStore);
    }
    if raw.is_null() {
        return Err(CredentialError::SecureStore);
    }
    let credential = unsafe { &*raw };
    let invalid = credential.CredentialBlobSize as usize > MAX_SECRET_BYTES
        || (credential.CredentialBlobSize > 0 && credential.CredentialBlob.is_null());
    if invalid {
        unsafe { CredFree(raw.cast()) };
        return Err(CredentialError::SecureStore);
    }
    let bytes = if credential.CredentialBlobSize == 0 {
        Vec::new()
    } else {
        unsafe {
            std::slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            )
            .to_vec()
        }
    };
    unsafe { CredFree(raw.cast()) };
    let value = match String::from_utf8(bytes) {
        Ok(value) => value,
        Err(error) => {
            let mut invalid = error.into_bytes();
            invalid.zeroize();
            return Err(CredentialError::SecureStore);
        }
    };
    SecretValue::new(value).map(Some)
}

pub(super) fn delete(locator: &CredentialLocator) -> Result<(), CredentialError> {
    use windows::core::{HRESULT, PCWSTR};
    use windows::Win32::Foundation::ERROR_NOT_FOUND;
    use windows::Win32::Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC};

    let target = wide_null(&locator.target_name());
    match unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None) } {
        Ok(()) => Ok(()),
        Err(error) if error.code() == HRESULT::from_win32(ERROR_NOT_FOUND.0) => Ok(()),
        Err(_) => Err(CredentialError::SecureStore),
    }
}
