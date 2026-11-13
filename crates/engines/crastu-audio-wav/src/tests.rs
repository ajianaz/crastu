use super::*;

/// Builds a WAV file in memory. `samples` are raw little-endian sample bytes.
fn wav(tag: u16, channels: u16, rate: u32, bits: u16, data: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&tag.to_le_bytes());
    v.extend_from_slice(&channels.to_le_bytes());
    v.extend_from_slice(&rate.to_le_bytes());
    let block = channels * bits / 8;
    v.extend_from_slice(&rate.wrapping_mul(u32::from(block)).to_le_bytes());
    v.extend_from_slice(&block.to_le_bytes());
    v.extend_from_slice(&bits.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&(data.len() as u32).to_le_bytes());
    v.extend_from_slice(data);
    v
}

fn pcm16(xs: &[i16]) -> Vec<u8> {
    xs.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn drain(src: &mut dyn AudioSource) -> Vec<f32> {
    let mut all = Vec::new();
    let mut buf = [0.0f32; 700];
    while let ReadResult::Samples(n) = src.read(&mut buf).unwrap() {
        all.extend_from_slice(&buf[..n]);
    }
    all
}

fn read_all(bytes: Vec<u8>) -> Vec<f32> {
    let mut s = FileSource::from_bytes(bytes).unwrap();
    s.start().unwrap();
    drain(&mut s)
}

#[test]
fn pcm16_mono() {
    let f = wav(1, 1, 16_000, 16, &pcm16(&[0, 16384, -16384, -32768]));
    let info = parse_wav(&f).unwrap();
    assert_eq!(
        (info.channels, info.sample_rate, info.frames()),
        (1, 16_000, 4)
    );
    assert_eq!(read_all(f), vec![0.0, 0.5, -0.5, -1.0]);
}

#[test]
fn pcm16_stereo_is_averaged() {
    let f = wav(1, 2, 44_100, 16, &pcm16(&[16384, 0, 16384, -16384]));
    let s = FileSource::from_bytes(f.clone()).unwrap();
    assert_eq!(s.sample_rate(), 44_100);
    assert_eq!(read_all(f), vec![0.25, 0.0]);
}

#[test]
fn float32_and_24bit_and_8bit() {
    let fl: Vec<u8> = [0.25f32, -0.5]
        .iter()
        .flat_map(|x| x.to_le_bytes())
        .collect();
    assert_eq!(read_all(wav(3, 1, 8000, 32, &fl)), vec![0.25, -0.5]);
    // 24-bit: 0x400000 = 0.5, 0xC00000 (negative) = -0.5
    let d24 = [0x00, 0x00, 0x40, 0x00, 0x00, 0xC0];
    assert_eq!(read_all(wav(1, 1, 8000, 24, &d24)), vec![0.5, -0.5]);
    assert_eq!(
        read_all(wav(1, 1, 8000, 8, &[128, 192, 0])),
        vec![0.0, 0.5, -1.0]
    );
}

#[test]
fn small_reads_cover_the_file_once() {
    let f = wav(
        1,
        1,
        8000,
        16,
        &pcm16(&(0..2000).map(|i| i as i16).collect::<Vec<_>>()),
    );
    let mut s = FileSource::from_bytes(f).unwrap();
    s.start().unwrap();
    let x = drain(&mut s); // drain reads 700 at a time
    assert_eq!(x.len(), 2000);
    assert_eq!(x[1999], 1999.0 / 32768.0);
}

#[test]
fn extra_chunks_and_odd_padding_are_skipped() {
    let mut f = wav(1, 1, 8000, 16, &pcm16(&[8192]));
    let mut g = f[..12].to_vec();
    g.extend_from_slice(b"LIST");
    g.extend_from_slice(&3u32.to_le_bytes());
    g.extend_from_slice(&[1, 2, 3, 0]); // 3 bytes + 1 pad
    g.extend_from_slice(&f.split_off(12));
    assert_eq!(read_all(g), vec![0.25]);
}

#[test]
fn streaming_header_sizes_mean_to_the_end() {
    let mut f = wav(1, 1, 8000, 16, &pcm16(&[8192, 8192]));
    let at = f.len() - 4 - 4;
    f[at..at + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    assert_eq!(read_all(f).len(), 2);
}

#[test]
fn errors() {
    assert!(matches!(parse_wav(b"nope"), Err(AudioError::Wav(_))));
    assert!(matches!(
        parse_wav(b"RIFF\0\0\0\0AVI LIST"),
        Err(AudioError::Wav(_))
    ));
    let good = wav(1, 1, 8000, 16, &pcm16(&[1, 2, 3, 4]));
    // Cut in the middle of the data: the header still claims 8 bytes.
    let e = parse_wav(&good[..good.len() - 3]).unwrap_err();
    assert_eq!(e, AudioError::Wav("data chunk is truncated".into()));
    // Cut before the data chunk.
    assert!(matches!(parse_wav(&good[..36]), Err(AudioError::Wav(_))));
    assert!(matches!(parse_wav(&good[..20]), Err(AudioError::Wav(_))));
    // 12-bit PCM and ADPCM are not read.
    assert!(matches!(
        parse_wav(&wav(1, 1, 8000, 12, &[0; 4])),
        Err(AudioError::UnsupportedFormat(_))
    ));
    assert!(matches!(
        parse_wav(&wav(2, 1, 8000, 4, &[0; 4])),
        Err(AudioError::UnsupportedFormat(_))
    ));
    // Zero channels.
    assert!(matches!(
        parse_wav(&wav(1, 0, 8000, 16, &[0; 4])),
        Err(AudioError::Wav(_))
    ));
    assert!(FileSource::open(Path::new("/nonexistent/x.wav")).is_err());
}

#[test]
fn absurd_sample_rates_are_errors_and_cheap() {
    for rate in [0u32, 384_001, 2_000_000_000, u32::MAX] {
        let f = wav(1, 1, rate, 16, &pcm16(&[1, 2]));
        let t = std::time::Instant::now();
        assert_eq!(parse_wav(&f).unwrap_err(), AudioError::BadRate(rate));
        assert!(FileSource::from_bytes(f).is_err());
        assert!(t.elapsed().as_millis() < 100);
    }
    assert!(parse_wav(&wav(1, 1, 384_000, 16, &pcm16(&[1]))).is_ok());
}

#[test]
fn an_empty_data_chunk_is_empty_even_with_chunks_after_it() {
    let mut f = wav(1, 1, 8000, 16, &[]);
    f.extend_from_slice(b"LIST\x04\0\0\0abcd");
    assert_eq!(parse_wav(&f).unwrap().frames(), 0);
    assert!(read_all(f).is_empty());
}

#[test]
fn nan_and_infinity_become_silence() {
    let d: Vec<u8> = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.5]
        .iter()
        .flat_map(|x| x.to_le_bytes())
        .collect();
    assert_eq!(read_all(wav(3, 1, 8000, 32, &d)), vec![0.0, 0.0, 0.0, 0.5]);
    let d: Vec<u8> = [f64::NAN, 0.25]
        .iter()
        .flat_map(|x| x.to_le_bytes())
        .collect();
    assert_eq!(read_all(wav(3, 1, 8000, 64, &d)), vec![0.0, 0.25]);
}

#[test]
fn int32_and_float64() {
    let d: Vec<u8> = [i32::MIN, 1 << 30]
        .iter()
        .flat_map(|x| x.to_le_bytes())
        .collect();
    assert_eq!(read_all(wav(1, 1, 8000, 32, &d)), vec![-1.0, 0.5]);
    let d: Vec<u8> = [-0.75f64, 0.125]
        .iter()
        .flat_map(|x| x.to_le_bytes())
        .collect();
    assert_eq!(read_all(wav(3, 1, 8000, 64, &d)), vec![-0.75, 0.125]);
}

#[test]
fn wave_format_extensible() {
    let mut v = Vec::new();
    v.extend_from_slice(b"RIFF\0\0\0\0WAVEfmt ");
    v.extend_from_slice(&40u32.to_le_bytes());
    v.extend_from_slice(&0xFFFEu16.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes()); // channels
    v.extend_from_slice(&16_000u32.to_le_bytes());
    v.extend_from_slice(&32_000u32.to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(&22u16.to_le_bytes()); // cbSize
    v.extend_from_slice(&16u16.to_le_bytes()); // valid bits
    v.extend_from_slice(&4u32.to_le_bytes()); // channel mask
    v.extend_from_slice(&1u16.to_le_bytes()); // sub-format: PCM
    v.extend_from_slice(&[0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xAA, 0, 0x38, 0x9B, 0x71]);
    v.extend_from_slice(b"data");
    v.extend_from_slice(&2u32.to_le_bytes());
    v.extend_from_slice(&16384i16.to_le_bytes());
    let info = parse_wav(&v).unwrap();
    assert_eq!(
        (info.sample_rate, info.bits, info.float),
        (16_000, 16, false)
    );
    assert_eq!(read_all(v.clone()), vec![0.5]);
    // An extensible fmt chunk too short to hold the sub-format is an error.
    let mut short = v;
    short[16..20].copy_from_slice(&18u32.to_le_bytes());
    assert!(parse_wav(&short).is_err());
}

#[test]
fn truncated_data_after_extra_chunks_is_an_error() {
    let mut f = wav(1, 1, 8000, 16, &pcm16(&[1, 2, 3, 4]));
    let mut g = f[..12].to_vec();
    g.extend_from_slice(b"LIST\x02\0\0\0ab");
    g.extend_from_slice(&f.split_off(12));
    let cut = g.len() - 2;
    assert_eq!(
        parse_wav(&g[..cut]).unwrap_err(),
        AudioError::Wav("data chunk is truncated".into())
    );
    assert!(parse_wav(&g).is_ok());
}

#[test]
fn hostile_chunk_sizes_do_not_overflow() {
    // A LIST chunk claiming 0xFFFFFFFF bytes, then nothing: an error, never a panic (the
    // size arithmetic is in u64, so it holds on 32-bit targets too).
    let mut f = wav(1, 1, 8000, 16, &pcm16(&[1]))[..36].to_vec();
    f.extend_from_slice(b"LIST\xFF\xFF\xFF\xFF");
    assert!(parse_wav(&f).is_err());
    let mut g = b"RIFF\0\0\0\0WAVEfmt \xFF\xFF\xFF\xFF".to_vec();
    g.extend_from_slice(&[0; 32]);
    assert!(parse_wav(&g).is_err());
}

#[test]
fn the_file_size_limit_is_enforced() {
    let dir = std::env::temp_dir().join(format!("crastu-audio-wav-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("a.wav");
    let f = wav(1, 1, 8000, 16, &pcm16(&[1, 2, 3, 4]));
    std::fs::write(&path, &f).unwrap();
    assert!(FileSource::open_with_limit(&path, f.len() as u64).is_ok());
    let e = FileSource::open_with_limit(&path, f.len() as u64 - 1).unwrap_err();
    assert!(matches!(e, AudioError::Wav(_)), "{e}");
    assert!(FileSource::open(&path).is_ok());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_partial_last_frame_is_ignored() {
    let f = wav(1, 2, 8000, 16, &[0, 0, 0, 0, 0, 0]); // 1.5 stereo frames
    assert_eq!(parse_wav(&f).unwrap().frames(), 1);
}

#[test]
fn an_empty_buffer_is_not_the_end() {
    let f = wav(1, 1, 8000, 16, &pcm16(&[1, 2]));
    let mut s = FileSource::from_bytes(f).unwrap();
    s.start().unwrap();
    assert_eq!(s.read(&mut []), Ok(ReadResult::Samples(0)));
    assert_eq!(drain(&mut s).len(), 2);
    assert_eq!(s.read(&mut []), Ok(ReadResult::End));
}
