//! Every bound the codecs enforce.
//!
//! The defaults mirror nginx's own limits where the protocol has one (FastCGI
//! record content is a 16-bit length; uWSGI `datasize` is 16-bit; a FastCGI
//! stdin chunk is 32 KiB in nginx; a memcached key is at most 250 bytes) and
//! add conservative bounds where the protocol has none. Callers may lower them
//! per listener; the codecs never allocate beyond what the active [`Limits`]
//! allows.

/// Hard bounds applied by the codecs in this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum encoded FastCGI `PARAMS` payload for one request.
    pub max_params_bytes: usize,
    /// Maximum number of `PARAMS` entries for one request.
    pub max_params: usize,
    /// Maximum number of entries in a parsed response header block.
    pub max_header_count: usize,
    /// Maximum size of one upstream response header block.
    pub max_header_block_bytes: usize,
    /// Maximum length of one response header line.
    pub max_header_line_bytes: usize,
    /// Maximum FastCGI `STDIN` chunk emitted by the request encoder.
    pub max_stdin_chunk_bytes: usize,
    /// Maximum size of one uWSGI packet body (16-bit `datasize`).
    pub max_uwsgi_packet_bytes: usize,
    /// Maximum size of one SCGI netstring header.
    pub max_scgi_header_bytes: usize,
    /// Maximum memcached key length (memcached protocol limit).
    pub max_memcached_key_bytes: usize,
    /// Maximum memcached value length.
    pub max_memcached_value_bytes: usize,
    /// Maximum number of gRPC metadata headers in one request.
    pub max_grpc_metadata: usize,
    /// Maximum size of one gRPC message (after the 5-byte frame header).
    pub max_grpc_message_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_params_bytes: 64 * 1024,
            max_params: 512,
            max_header_count: 256,
            max_header_block_bytes: 32 * 1024,
            max_header_line_bytes: 8 * 1024,
            max_stdin_chunk_bytes: 32 * 1024,
            max_uwsgi_packet_bytes: u16::MAX as usize,
            max_scgi_header_bytes: 64 * 1024,
            max_memcached_key_bytes: 250,
            max_memcached_value_bytes: 1024 * 1024,
            max_grpc_metadata: 64,
            max_grpc_message_bytes: 4 * 1024 * 1024,
        }
    }
}

impl Limits {
    /// Clamp every field to a non-zero value so a zeroed configuration cannot
    /// disable the bounds entirely.
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        for field in [
            &mut self.max_params_bytes,
            &mut self.max_params,
            &mut self.max_header_count,
            &mut self.max_header_block_bytes,
            &mut self.max_header_line_bytes,
            &mut self.max_stdin_chunk_bytes,
            &mut self.max_uwsgi_packet_bytes,
            &mut self.max_scgi_header_bytes,
            &mut self.max_memcached_key_bytes,
            &mut self.max_memcached_value_bytes,
            &mut self.max_grpc_metadata,
            &mut self.max_grpc_message_bytes,
        ] {
            if *field == 0 {
                *field = 1;
            }
        }
        // The wire fields that carry a 16-bit size can never be configured
        // above 65535, whatever the caller passes.
        self.max_uwsgi_packet_bytes = self.max_uwsgi_packet_bytes.min(u16::MAX as usize);
        self.max_stdin_chunk_bytes = self.max_stdin_chunk_bytes.min(u16::MAX as usize);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::Limits;

    #[test]
    fn sanitized_bounds_zero_fields_to_one() {
        let limits = Limits {
            max_params_bytes: 0,
            max_params: 0,
            max_header_count: 0,
            max_header_block_bytes: 0,
            max_header_line_bytes: 0,
            max_stdin_chunk_bytes: 0,
            max_uwsgi_packet_bytes: 0,
            max_scgi_header_bytes: 0,
            max_memcached_key_bytes: 0,
            max_memcached_value_bytes: 0,
            max_grpc_metadata: 0,
            max_grpc_message_bytes: 0,
        }
        .sanitized();
        assert_eq!(limits.max_params, 1);
        assert_eq!(limits.max_grpc_message_bytes, 1);
        assert_eq!(limits.max_stdin_chunk_bytes, 1);
    }

    #[test]
    fn sanitized_bounds_16_bit_wire_fields() {
        let limits = Limits {
            max_stdin_chunk_bytes: usize::MAX,
            max_uwsgi_packet_bytes: usize::MAX,
            ..Limits::default()
        }
        .sanitized();
        assert_eq!(limits.max_stdin_chunk_bytes, u16::MAX as usize);
        assert_eq!(limits.max_uwsgi_packet_bytes, u16::MAX as usize);
    }
}
