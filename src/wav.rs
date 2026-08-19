//! WAV container (SPEC §9.7): 44-byte canonical PCM header followed by
//! `frames × 4` data bytes — 44100 Hz, stereo, s16le.

/// The container's hard ceiling: `data_size` and `36 + data_size` are
/// u32 fields (SPEC §9.7), so at 4 bytes/frame at most
/// (2^32 − 1 − 36) / 4 frames (≈ 6.76 hours) fit. Render enforces it
/// with a clean error (SPEC-GAPS #9).
pub const MAX_FRAMES: usize = ((u32::MAX as usize) - 36) / 4;

/// The exact 44-byte header for a stereo s16 stream of `frames` frames.
pub fn header(frames: usize) -> [u8; 44] {
    assert!(frames <= MAX_FRAMES, "frame count exceeds WAV u32 sizes");
    let data_size = (frames * 4) as u32;
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data_size).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    h[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    h[22..24].copy_from_slice(&2u16.to_le_bytes()); // channels
    h[24..28].copy_from_slice(&44100u32.to_le_bytes()); // sample rate
    h[28..32].copy_from_slice(&176400u32.to_le_bytes()); // byte rate
    h[32..34].copy_from_slice(&4u16.to_le_bytes()); // block align
    h[34..36].copy_from_slice(&16u16.to_le_bytes()); // bits per sample
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_size.to_le_bytes());
    h
}

/// Full file bytes: header + interleaved L-then-R little-endian i16.
pub fn file_bytes(frames: &[(i16, i16)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(44 + frames.len() * 4);
    out.extend_from_slice(&header(frames.len()));
    for (l, r) in frames {
        out.extend_from_slice(&l.to_le_bytes());
        out.extend_from_slice(&r.to_le_bytes());
    }
    out
}
