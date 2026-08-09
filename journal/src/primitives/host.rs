#[cfg(feature = "wasm-kernel")]
use crate::*;

#[cfg(feature = "wasm-kernel")]
unsafe extern "C" {
    fn sync_web_host_capability(
        operation: u32,
        request: *const u8,
        request_len: usize,
        response: *mut u8,
        response_cap: usize,
    ) -> isize;
}

#[cfg(feature = "wasm-kernel")]
pub(crate) unsafe fn kernel_capability_bytes(
    sc: *mut s7::s7_scheme,
    operation: u32,
    request: &[u8],
    external: bool,
) -> Result<Vec<u8>, s7::s7_pointer> {
    unsafe {
        if external {
            mark_external_called(sc);
        }
        const CAPABILITY_LIMIT: usize = 16 * 1024 * 1024;
        let mut response = vec![0_u8; 4096];
        let mut length = sync_web_host_capability(
            operation,
            request.as_ptr(),
            request.len(),
            response.as_mut_ptr(),
            response.len(),
        );
        if length < 0 || length as usize > CAPABILITY_LIMIT {
            return Err(sync_error(sc, "bounded native capability failed"));
        }
        if length as usize > response.len() {
            response.resize(length as usize, 0);
            length = sync_web_host_capability(
                operation,
                request.as_ptr(),
                request.len(),
                response.as_mut_ptr(),
                response.len(),
            );
        }
        if length < 0 || length as usize > response.len() {
            return Err(sync_error(sc, "bounded native capability failed"));
        }
        response.truncate(length as usize);
        if operation == 6 {
            let mut ignored = [0_u8; 4];
            let _ = sync_web_host_capability(
                7,
                request.as_ptr(),
                request.len(),
                ignored.as_mut_ptr(),
                ignored.len(),
            );
        }
        Ok(response)
    }
}

#[cfg(feature = "wasm-kernel")]
pub(crate) unsafe fn kernel_capability(
    sc: *mut s7::s7_scheme,
    operation: u32,
    request: &[u8],
    external: bool,
) -> s7::s7_pointer {
    unsafe {
        let response = match kernel_capability_bytes(sc, operation, request, external) {
            Ok(response) => response,
            Err(error) => return error,
        };
        let Ok(response) = CString::new(response) else {
            return sync_error(sc, "native capability returned invalid data");
        };
        s7::s7_eval_c_string(sc, response.as_ptr())
    }
}
