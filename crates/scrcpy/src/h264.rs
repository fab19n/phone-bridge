//! H.264 helpers: NAL unit iteration over Annex-B data and the WebCodecs codec string
//! (`protocol/video.md` §13, REVIEW B3).

/// `nal_unit_type` of a sequence parameter set.
pub const NAL_TYPE_SPS: u8 = 7;
/// `nal_unit_type` of a picture parameter set.
pub const NAL_TYPE_PPS: u8 = 8;

/// The NAL units in an Annex-B buffer (start codes removed).
///
/// Units are split at every `00 00 01`. Zero bytes at the end of a unit are trimmed: they are the
/// extra zero of a 4-byte start code or `trailing_zero_8bits`, never part of a NAL unit (H.264
/// Annex B). Bytes before the first start code are skipped, so data without a start code has no
/// units, and empty units are dropped.
pub fn nal_units(annexb: &[u8]) -> Vec<&[u8]> {
    let mut units = Vec::new();
    let Some(mut start) = find_start_code(annexb, 0).map(|at| at + 3) else {
        return units;
    };
    while let Some(next) = find_start_code(annexb, start) {
        push_unit(&mut units, &annexb[start..next]);
        start = next + 3;
    }
    push_unit(&mut units, &annexb[start..]);
    units
}

/// `nal_unit_type` (the low 5 bits of the first byte).
pub fn nal_type(nal: &[u8]) -> Option<u8> {
    nal.first().map(|b| b & 0x1F)
}

/// `avc1.PPCCLL` from the first SPS (type 7) in an Annex-B buffer: `profile_idc`, the
/// constraint-flags byte and `level_idc`, in uppercase hex. `None` if there is no usable SPS.
pub fn codec_string(annexb: &[u8]) -> Option<String> {
    let sps = nal_units(annexb)
        .into_iter()
        .find(|nal| nal_type(nal) == Some(NAL_TYPE_SPS))?;
    // The bytes after the NAL header; no emulation prevention byte can occur this early, since
    // profile_idc is never 0.
    let &[_, profile_idc, constraint_flags, level_idc, ..] = sps else {
        return None;
    };
    Some(format!(
        "avc1.{profile_idc:02X}{constraint_flags:02X}{level_idc:02X}"
    ))
}

/// The index of the next `00 00 01` at or after `from`.
fn find_start_code(data: &[u8], from: usize) -> Option<usize> {
    let at = data.get(from..)?.windows(3).position(|w| w == [0, 0, 1])?;
    Some(from + at)
}

fn push_unit<'a>(units: &mut Vec<&'a [u8]>, unit: &'a [u8]) {
    if let Some(last) = unit.iter().rposition(|&b| b != 0) {
        units.push(&unit[..=last]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Parameter sets as encoders emit them (the bytes after level_idc are only for realism).
    const SPS_BASELINE_31: &[u8] = &[
        0x67, 0x42, 0xe0, 0x1f, 0xda, 0x01, 0x40, 0x16, 0xe8, 0x06, 0xd0, 0xa1, 0x35,
    ];
    const SPS_MAIN_31: &[u8] = &[
        0x67, 0x4d, 0x40, 0x1f, 0xe8, 0x80, 0x28, 0x02, 0xdd, 0x80, 0xb5, 0x01,
    ];
    const SPS_HIGH_41: &[u8] = &[
        0x67, 0x64, 0x00, 0x29, 0xac, 0x2b, 0x40, 0x3c, 0x01, 0x13, 0xf2, 0xe0,
    ];
    const PPS: &[u8] = &[0x68, 0xee, 0x3c, 0x80];
    const IDR: &[u8] = &[0x65, 0x88, 0x84, 0x00, 0x33, 0xff];

    fn annexb(start_code: &[u8], nals: &[&[u8]]) -> Vec<u8> {
        nals.iter()
            .flat_map(|nal| [start_code, nal])
            .flatten()
            .copied()
            .collect()
    }

    #[test]
    fn splits_on_4_byte_start_codes() {
        let data = annexb(&[0, 0, 0, 1], &[SPS_HIGH_41, PPS, IDR]);
        assert_eq!(nal_units(&data), [SPS_HIGH_41, PPS, IDR]);
    }

    #[test]
    fn splits_on_3_byte_and_mixed_start_codes() {
        let data = annexb(&[0, 0, 1], &[SPS_MAIN_31, PPS, IDR]);
        assert_eq!(nal_units(&data), [SPS_MAIN_31, PPS, IDR]);

        let mut data = vec![0, 0, 1];
        data.extend_from_slice(SPS_MAIN_31);
        data.extend_from_slice(&[0, 0, 0, 1]);
        data.extend_from_slice(PPS);
        data.extend_from_slice(&[0, 0, 1]);
        data.extend_from_slice(IDR);
        assert_eq!(nal_units(&data), [SPS_MAIN_31, PPS, IDR]);
    }

    #[test]
    fn trims_trailing_zero_bytes() {
        let mut data = annexb(&[0, 0, 0, 1], &[SPS_HIGH_41, PPS]);
        data.extend_from_slice(&[0, 0, 0]);
        assert_eq!(nal_units(&data), [SPS_HIGH_41, PPS]);

        // trailing_zero_8bits between units; zero bytes inside a unit stay.
        let data = [
            0, 0, 1, 0x09, 0xf0, 0, 0, 0, 0, 0, 1, 0x67, 0x64, 0, 3, 0, 0, 1, 0x68,
        ];
        assert_eq!(
            nal_units(&data),
            [&[0x09, 0xf0][..], &[0x67, 0x64, 0, 3], &[0x68]]
        );
    }

    #[test]
    fn edge_cases_have_no_units() {
        assert!(nal_units(&[]).is_empty());
        assert!(nal_units(&[0, 0, 0, 0]).is_empty());
        assert!(nal_units(SPS_HIGH_41).is_empty(), "no start code");
        assert!(nal_units(&[0, 0, 1]).is_empty());
        assert!(nal_units(&[0, 0, 0, 1, 0, 0, 0, 1, 0, 0]).is_empty());
        assert!(
            nal_units(&[0, 1, 0x67, 0x64]).is_empty(),
            "2-byte prefix is not a start code"
        );
    }

    #[test]
    fn skips_leading_bytes_and_empty_units() {
        let data = [
            0xde, 0xad, 0, 0, 1, 0x67, 0x42, 0, 0, 1, 0, 0, 1, 0x68, 0xce,
        ];
        assert_eq!(nal_units(&data), [&[0x67, 0x42][..], &[0x68, 0xce]]);
    }

    #[test]
    fn nal_types() {
        assert_eq!(nal_type(SPS_HIGH_41), Some(NAL_TYPE_SPS));
        assert_eq!(nal_type(PPS), Some(NAL_TYPE_PPS));
        assert_eq!(nal_type(IDR), Some(5));
        assert_eq!(
            nal_type(&[0x27]),
            Some(7),
            "nal_ref_idc is not part of the type"
        );
        assert_eq!(nal_type(&[]), None);
    }

    #[test]
    fn codec_strings_for_common_profiles() {
        let start = [0, 0, 0, 1];
        assert_eq!(
            codec_string(&annexb(&start, &[SPS_BASELINE_31, PPS])).as_deref(),
            Some("avc1.42E01F")
        );
        assert_eq!(
            codec_string(&annexb(&start, &[SPS_MAIN_31, PPS])).as_deref(),
            Some("avc1.4D401F")
        );
        assert_eq!(
            codec_string(&annexb(&start, &[SPS_HIGH_41, PPS, IDR])).as_deref(),
            Some("avc1.640029")
        );
    }

    #[test]
    fn codec_string_uses_the_first_sps_anywhere_in_the_buffer() {
        let aud: &[u8] = &[0x09, 0xf0];
        let data = annexb(&[0, 0, 1], &[aud, PPS, SPS_MAIN_31, SPS_HIGH_41, IDR]);
        assert_eq!(codec_string(&data).as_deref(), Some("avc1.4D401F"));

        let sps_ref_idc_1 = [0x27, 0x64, 0x00, 0x28];
        assert_eq!(
            codec_string(&annexb(&[0, 0, 1], &[&sps_ref_idc_1])).as_deref(),
            Some("avc1.640028")
        );
    }

    #[test]
    fn codec_string_needs_a_complete_sps() {
        assert_eq!(codec_string(&[]), None);
        assert_eq!(codec_string(SPS_HIGH_41), None, "no start code");
        assert_eq!(
            codec_string(&annexb(&[0, 0, 0, 1], &[PPS, IDR])),
            None,
            "no SPS"
        );
        assert_eq!(
            codec_string(&[0, 0, 0, 1, 0x67, 0x64, 0x00]),
            None,
            "truncated SPS"
        );
    }
}
