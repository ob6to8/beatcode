//! SPEC §11.3 item 6: sha256 green on FIPS 180-4 standard vectors;
//! WAV header bytes match §9.7.

use bc::{sha256, wav};

#[test]
fn sha256_fips_vectors() {
    // FIPS 180-4 / NIST CAVS standard vectors.
    let cases: &[(&[u8], &str)] = &[
        (
            b"",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            b"abc",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        ),
        (
            b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmno\
              ijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu",
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
        ),
    ];
    for (input, want) in cases {
        assert_eq!(&sha256::hex(input), want, "sha256({input:?})");
    }

    // One million 'a's.
    let million = vec![b'a'; 1_000_000];
    assert_eq!(
        sha256::hex(&million),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );

    // Streaming in ragged chunks must equal one-shot hashing.
    let mut h = bc::sha256::Sha256::new();
    for chunk in million.chunks(97) {
        h.update(chunk);
    }
    let digest = h.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        hex,
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

/// SPEC §9.7's reference header hex: the first 44 bytes of a
/// 116579-frame render (data_size = 466316 = 0x00071d8c). Sizes depend
/// on event content; the layout is the golden.
#[test]
fn wav_header_reference_bytes() {
    let want: [u8; 44] = [
        0x52, 0x49, 0x46, 0x46, 0xb0, 0x1d, 0x07, 0x00, 0x57, 0x41, 0x56, 0x45, 0x66, 0x6d, 0x74,
        0x20, 0x10, 0x00, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00, 0x44, 0xac, 0x00, 0x00, 0x10, 0xb1,
        0x02, 0x00, 0x04, 0x00, 0x10, 0x00, 0x64, 0x61, 0x74, 0x61, 0x8c, 0x1d, 0x07, 0x00,
    ];
    assert_eq!(wav::header(116579), want);
}

/// §11.1 property 6: WAV header sanity on an actual render.
#[test]
fn wav_header_sanity_on_render() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/four.bc"))
        .expect("read four.bc");
    let sc = bc::score::parse(&src).expect("parse");
    let evs = bc::events::compile(&sc).expect("compile");
    let r = bc::render::render(&evs).expect("render");
    let b = &r.wav_bytes;
    assert_eq!(&b[0..4], b"RIFF");
    assert_eq!(&b[8..12], b"WAVE");
    assert_eq!(&b[12..16], b"fmt ");
    assert_eq!(&b[36..40], b"data");
    let data_size = u32::from_le_bytes(b[40..44].try_into().expect("4")) as usize;
    assert_eq!(data_size, r.frames * 4);
    assert_eq!(b.len(), 44 + data_size);
    let riff_size = u32::from_le_bytes(b[4..8].try_into().expect("4")) as usize;
    assert_eq!(riff_size, 36 + data_size);
    assert_eq!(u32::from_le_bytes(b[24..28].try_into().expect("4")), 44100);
    assert_eq!(u32::from_le_bytes(b[28..32].try_into().expect("4")), 176400);
}
