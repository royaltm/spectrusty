use core::iter::StepBy;
use core::ops::Range;

#[cfg(feature = "snapshot")]
use serde::{Serialize, Deserialize};

use crate::clock::{VideoTs, Ts};
use crate::video::{BorderSize, VideoFrame, CellCoords};
use super::UlaVideoFrame;

#[derive(Clone, Copy, Default, Debug, PartialEq)]
#[cfg_attr(feature = "snapshot", derive(Serialize, Deserialize))]
pub struct UlaNTSCVidFrame;

impl VideoFrame for UlaNTSCVidFrame {
    /// A range of horizontal T-states, 0 should be when the frame starts.
    const HTS_RANGE: Range<Ts> = UlaVideoFrame::HTS_RANGE;
    /// The first video scan line index of the top border.
    const VSL_BORDER_TOP: Ts = 16;
    /// A range of video scan line indexes for the pixel area.
    const VSL_PIXELS: Range<Ts> = 40..232;
    /// The last video scan line index of the bottom border.
    const VSL_BORDER_BOT: Ts = 256;
    /// A total number of video scan lines.
    const VSL_COUNT: Ts = 264;

    type BorderHtsIter = StepBy<Range<Ts>>;

    fn border_whole_line_hts_iter(border_size: BorderSize) -> Self::BorderHtsIter {
        UlaVideoFrame::border_whole_line_hts_iter(border_size)
    }

    fn border_left_hts_iter(border_size: BorderSize) -> Self::BorderHtsIter {
        UlaVideoFrame::border_left_hts_iter(border_size)
    }

    fn border_right_hts_iter(border_size: BorderSize) -> Self::BorderHtsIter {
        UlaVideoFrame::border_right_hts_iter(border_size)
    }

    #[inline]
    fn contention(hc: Ts) -> Ts {
        UlaVideoFrame::contention(hc)
    }

    #[inline(always)]
    fn floating_bus_offset(hc: Ts) -> Option<u16> {
        UlaVideoFrame::floating_bus_offset(hc)
    }

    #[inline(always)]
    fn snow_interference_coords(vts: VideoTs) -> Option<CellCoords> {
        UlaVideoFrame::snow_interference_coords(vts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    type TestVideoFrame = UlaNTSCVidFrame;

    #[test]
    fn test_contention() {
        let vts0 = VideoTs::new(0, 0);
        let tstates = [(8959, 8965),
                       (8960, 8965),
                       (8961, 8965),
                       (8962, 8965),
                       (8963, 8965),
                       (8964, 8965),
                       (8965, 8965),
                       (8966, 8966)];
        for offset in (0..16).map(|x| x * 8i32) {
            for (testing, target) in tstates.iter().copied() {
                let mut vts = TestVideoFrame::vts_add_ts(vts0, testing + offset as u32);
                vts.hc = TestVideoFrame::contention(vts.hc);
                assert_eq!(TestVideoFrame::normalize_vts(vts),
                           TestVideoFrame::tstates_to_vts(target + offset));
            }
        }
        let refts = tstates[0].0 as i32;
        for ts in (refts - 96..refts)
            .chain(refts + 128..refts+TestVideoFrame::HTS_COUNT as i32) {
            let vts = TestVideoFrame::tstates_to_vts(ts);
            assert_eq!(TestVideoFrame::contention(vts.hc), vts.hc);
        }
    }

    #[test]
    fn test_video_frame_vts_utils() {
        let items = [((  0, -69),   -69, ( 0, 59067), false, true , (  0, -69)),
                     ((  0,   0),     0, ( 1,     0), false, true , (  0,   0)),
                     ((  0,  -1),    -1, ( 0, 59135), false, true , (  0,  -1)),
                     (( -1,   0),  -224, ( 0, 58912), false, true , ( -1,   0)),
                     ((  1,   0),   224, ( 1,   224), false, true , (  1,   0)),
                     ((264,  -1), 59135, ( 1, 59135), true , true , (264,  -1)),
                     ((264,   0), 59136, ( 2,     0), true , true , (264,   0)),
                     ((  0, 224),   224, ( 1,   224), false, false, (  1,   0)),
                     ((528,-223),118049, ( 2, 58913), true,  false, (527,   1))];
        for ((vc, hc), fts, (nfr, nfts), eof, is_norm, (nvc, nhc)) in items.iter().copied() {
            let vts = VideoTs::new(vc, hc);
            let nvts = VideoTs::new(nvc, nhc);
            assert_eq!(TestVideoFrame::vc_hc_to_tstates(vc, hc), fts);
            assert_eq!(TestVideoFrame::vts_to_tstates(vts), fts);
            assert_eq!(TestVideoFrame::tstates_to_vts(fts), nvts);
            assert_eq!(TestVideoFrame::vts_to_norm_tstates(1, vts), (nfr, nfts));
            assert_eq!(TestVideoFrame::is_vts_eof(vts), eof);
            assert_eq!(TestVideoFrame::is_normalized_vts(vts), is_norm);
            assert_eq!(TestVideoFrame::normalize_vts(vts), nvts);
        }
        assert_eq!(TestVideoFrame::vts_max(), VideoTs::new(i16::max_value(), 154));
        assert_eq!(TestVideoFrame::vts_min(), VideoTs::new(i16::min_value(), -69));
        let items = [((  0,   0),     0, (  0,   0)),
                     ((  0,   0),     1, (  0,   1)),
                     (( -1, 154),     1, (  0, -69)),
                     ((  0,   0),   224, (  1,   0)),
                     (( -1,   1),   223, (  0,   0)),
                     ((  0,   0), 59136, (264,   0)),
                     ((  1,  -1), 59136, (265,  -1)),
                     ((  2, 224), 59136, (267,   0))];
        for ((vc0, hc0), delta, (vc1, hc1)) in items.iter().copied() {
            let vts0 = VideoTs::new(vc0, hc0);
            let vts1 = VideoTs::new(vc1, hc1);
            assert_eq!(TestVideoFrame::vts_add_ts(vts0, delta), vts1);
            assert_eq!(TestVideoFrame::vts_diff(vts0, vts1), delta as i32);
            assert_eq!(TestVideoFrame::vts_diff(vts1, vts0), -(delta as i32));
        }
        let items = [((   264,      0), (     0,      0)),
                     ((   264,    -69), (     0,    -69)),
                     ((   527,    154), (   263,    154)),
                     ((     0,    224), (  -264,    224)),
                     ((-32767, -32768), (-32768, -32768)),
                     ((-32768, -32768), (-32768, -32768))];
        for ((vc0, hc0), (vc1, hc1)) in items.iter().copied() {
            let vts0 = VideoTs::new(vc0, hc0);
            let vts1 = VideoTs::new(vc1, hc1);
            assert_eq!(TestVideoFrame::vts_saturating_sub_frame(vts0), vts1);
        }
        let items = [((     0,      0), (     0,      0), (     0,      0), (     0,      0)),
                     ((     1,      1), (     1,      1), (     0,      0), (     2,      2)),
                     ((     1,      1), (    -1,     -1), (     2,      2), (     0,      0)),
                     ((     1,    154), (     1,      1), (     0,    153), (     3,    -69)),
                     ((-32768,    -69), (     1,      1), (-32768,    -69), (-32767,    -68)),
                     ((-32768,    -69), (-32768,    -69), (     0,      0), (-32768,    -69)),
                     (( 32767,    154), (     1,      1), ( 32766,    153), ( 32767,    154)),
                     (( 32767,    154), ( 32767,    154), (     0,      0), ( 32767,    154))];
        for ((vc0, hc0), (vc1, hc1), (svc, shc), (avc, ahc)) in items.iter().copied() {
            let vts0 = VideoTs::new(vc0, hc0);
            let vts1 = VideoTs::new(vc1, hc1);
            let subvts = VideoTs::new(svc, shc);
            let addvts = VideoTs::new(avc, ahc);
            assert_eq!(TestVideoFrame::vts_saturating_sub_vts_normalized(vts0, vts1), subvts);
            assert_eq!(TestVideoFrame::vts_saturating_add_vts_normalized(vts0, vts1), addvts);
            assert_eq!(TestVideoFrame::vts_saturating_add_vts_normalized(vts1, vts0), addvts);
        }
    }
}
