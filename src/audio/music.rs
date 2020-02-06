//! Music related functions
use core::convert::TryInto;
/// Returns an iterator of equal tempered scale frequencies (in a single octave) from a given base frequency.
/// `hz` base frequency is in Hz (use 440.0 as a good default).
/// `n0` an index from the base frequency to the first note in the table: 0 is for "A" note, -9 for "C".
/// `steps` determines how many half tones will be rendered, (12 is the usual number).
pub fn equal_tempered_scale_note_freqs(hz: f32, n0: i16, steps: i16)
                                         -> impl IntoIterator<Item=f32> + Clone + ExactSizeIterator
{
    (0..steps).map(move |n| {
        hz * (2.0f32).powf( (n + n0) as f32 / steps as f32 )
    })
}

/// Renders an array of equal tempered scale frequencies (in a single octave) from a given base frequency.
/// `hz` base frequency is in Hz (use 440.0 as a good default).
/// `n0` an index from the base frequency to the first note in the table: 0 is for "A" note, -9 for "C".
/// The size of `target` determines how many half tones will be rendered, (12 is the usual number).
pub fn render_equal_tempered_scale_note_freqs(hz: f32, n0: i16, target: &mut [f32]) {
    let steps = target.len().try_into().expect("target is too large");
    for (t, hz) in target.iter_mut()
                   .zip(equal_tempered_scale_note_freqs(hz, n0, steps)) {
        *t = hz
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // use std::error;

    #[test]
    fn music_works() {
        let mut freqs = [0.0f32;12];
        render_equal_tempered_scale_note_freqs(440.0, 0, &mut freqs);
        assert_eq!(freqs,  [440.0, 466.1638, 493.8833, 523.2511, 554.3653, 587.3295,
                            622.25397, 659.2551, 698.4565, 739.98883, 783.99084, 830.6094])
    }
}
