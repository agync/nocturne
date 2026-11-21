//! Keeps with bytes, sent from the screens in slices. Android's web view carries no raw IPC
//! body, so one invoke with a whole photo would arrive as a JSON array of numbers; a slice of
//! base64url text at a time stays small. The bytes are put together here, then sent across.

use std::collections::HashMap;

use nocturne_channel::crypto;
use nocturne_channel::protocol::MAX_ITEM;
use nocturne_contract::RemoteCall;

/// The largest slice the screens send, before base64.
pub const MAX_SLICE: usize = 512 * 1024;
/// Keeps begun and not finished, at most; a new one past this drops the oldest.
const MAX_OPEN: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadError {
    /// Not a keep with bytes, a size out of range, a bad slice or too many bytes.
    Invalid,
    /// No keep with this handle (finished, cancelled or dropped).
    Unknown,
}

struct Open {
    call: RemoteCall,
    size: usize,
    bytes: Vec<u8>,
}

#[derive(Default)]
pub struct Uploads {
    next: u32,
    open: HashMap<u32, Open>,
}

impl Uploads {
    /// Start a keep; answers its handle.
    pub fn begin(&mut self, call: RemoteCall) -> Result<u32, UploadError> {
        let size = match &call {
            RemoteCall::KeepBytes { size, .. } | RemoteCall::KeepRecording { size, .. } => *size,
            _ => return Err(UploadError::Invalid),
        };
        if !(1.0..=MAX_ITEM as f64).contains(&size) || size.fract() != 0.0 {
            return Err(UploadError::Invalid);
        }
        if self.open.len() >= MAX_OPEN
            && let Some(oldest) = self.open.keys().min().copied()
        {
            self.open.remove(&oldest);
        }
        self.next = self.next.wrapping_add(1);
        let size = size as usize;
        self.open.insert(
            self.next,
            Open {
                call,
                size,
                bytes: Vec::with_capacity(size),
            },
        );
        Ok(self.next)
    }

    /// One slice, base64url without padding, in order.
    pub fn part(&mut self, handle: u32, slice: &str) -> Result<(), UploadError> {
        let open = self.open.get_mut(&handle).ok_or(UploadError::Unknown)?;
        let fail = |uploads: &mut Uploads| {
            uploads.open.remove(&handle);
            Err(UploadError::Invalid)
        };
        let Some(bytes) = crypto::unb64(slice) else {
            return fail(self);
        };
        if bytes.is_empty() || bytes.len() > MAX_SLICE || open.bytes.len() + bytes.len() > open.size
        {
            return fail(self);
        }
        open.bytes.extend_from_slice(&bytes);
        Ok(())
    }

    /// The call and its bytes, once every byte is in.
    pub fn end(&mut self, handle: u32) -> Result<(RemoteCall, Vec<u8>), UploadError> {
        let open = self.open.remove(&handle).ok_or(UploadError::Unknown)?;
        if open.bytes.len() != open.size {
            return Err(UploadError::Invalid);
        }
        Ok((open.call, open.bytes))
    }

    pub fn cancel(&mut self, handle: u32) {
        self.open.remove(&handle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(size: usize) -> RemoteCall {
        RemoteCall::KeepBytes {
            mime: "image/jpeg".into(),
            size: size as f64,
            client_id: "c-1".into(),
            caption: None,
            feeling: None,
            good: None,
            captured_at: None,
        }
    }

    #[test]
    fn twenty_megabytes_in_slices_come_back_whole() {
        let photo: Vec<u8> = (0..20 * 1024 * 1024)
            .map(|i| (i * 31 % 251) as u8)
            .collect();
        let mut u = Uploads::default();
        let h = u.begin(call(photo.len())).unwrap();
        for slice in photo.chunks(MAX_SLICE) {
            u.part(h, &crypto::b64(slice)).unwrap();
        }
        let (got, bytes) = u.end(h).unwrap();
        assert_eq!(got, call(photo.len()));
        assert!(bytes == photo);
        assert_eq!(u.end(h).err(), Some(UploadError::Unknown));
    }

    #[test]
    fn refuses_what_does_not_fit() {
        let mut u = Uploads::default();
        assert_eq!(
            u.begin(RemoteCall::GetStatus).err(),
            Some(UploadError::Invalid)
        );
        assert_eq!(u.begin(call(0)).err(), Some(UploadError::Invalid));
        assert_eq!(
            u.begin(call(MAX_ITEM as usize + 1)).err(),
            Some(UploadError::Invalid)
        );
        let h = u.begin(call(4)).unwrap();
        assert_eq!(
            u.part(h, &crypto::b64(&[1, 2, 3, 4, 5])).err(),
            Some(UploadError::Invalid)
        );
        assert_eq!(
            u.part(h, "AA").err(),
            Some(UploadError::Unknown),
            "a bad slice ends it"
        );
        let h = u.begin(call(4)).unwrap();
        u.part(h, &crypto::b64(&[1, 2])).unwrap();
        assert_eq!(
            u.end(h).err(),
            Some(UploadError::Invalid),
            "two bytes short"
        );
        let h = u.begin(call(4)).unwrap();
        assert_eq!(u.part(h, "not base64!").err(), Some(UploadError::Invalid));
        let big = vec![0u8; MAX_SLICE + 1];
        let h = u.begin(call(MAX_SLICE + 1)).unwrap();
        assert_eq!(
            u.part(h, &crypto::b64(&big)).err(),
            Some(UploadError::Invalid)
        );
    }

    #[test]
    fn keeps_only_a_few_open() {
        let mut u = Uploads::default();
        let first = u.begin(call(1)).unwrap();
        for _ in 0..MAX_OPEN {
            u.begin(call(1)).unwrap();
        }
        assert_eq!(u.part(first, "AA").err(), Some(UploadError::Unknown));
    }
}
