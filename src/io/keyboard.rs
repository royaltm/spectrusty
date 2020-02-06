bitflags! {
    #[derive(Default)]
    pub struct ZXKeyboardMap: u64 {
        const V  = 0x0000000001;
        const G  = 0x0000000002;
        const T  = 0x0000000004;
        const N5 = 0x0000000008;
        const N6 = 0x0000000010;
        const Y  = 0x0000000020;
        const H  = 0x0000000040;
        const B  = 0x0000000080;
        const C  = 0x0000000100;
        const F  = 0x0000000200;
        const R  = 0x0000000400;
        const N4 = 0x0000000800;
        const N7 = 0x0000001000;
        const U  = 0x0000002000;
        const J  = 0x0000004000;
        const N  = 0x0000008000;
        const X  = 0x0000010000;
        const D  = 0x0000020000;
        const E  = 0x0000040000;
        const N3 = 0x0000080000;
        const N8 = 0x0000100000;
        const I  = 0x0000200000;
        const K  = 0x0000400000;
        const M  = 0x0000800000;
        const Z  = 0x0001000000;
        const S  = 0x0002000000;
        const W  = 0x0004000000;
        const N2 = 0x0008000000;
        const N9 = 0x0010000000;
        const O  = 0x0020000000;
        const L  = 0x0040000000;
        const SS = 0x0080000000;
        const CS = 0x0100000000;
        const A  = 0x0200000000;
        const Q  = 0x0400000000;
        const N1 = 0x0800000000;
        const N0 = 0x1000000000;
        const P  = 0x2000000000;
        const EN = 0x4000000000;
        const BR = 0x8000000000;
    }
}

pub trait KeyboardInterface {
    fn get_key_state(&self) -> ZXKeyboardMap;
    fn set_key_state(&mut self, keymap: ZXKeyboardMap);
}

impl ZXKeyboardMap {
    /// See: http://rk.nvg.ntnu.no/sinclair/computers/zxspectrum/spec48versions.htm#issue1
    /// ```text
    /// keyboard   x   7f bf df ef f7 fb fd fe   keyboard[n] & !m == !m key is pressed
    ///           [0]   B  H  Y  6  5  T  G  V
    ///           [1]   N  J  U  7  4  R  F  C
    ///           [2]   M  K  I  8  3  E  D  X
    ///           [3]  SS  L  O  9  2  W  S  Z
    ///           [4]  BR EN  P  0  1  Q  A CS
    /// INNER SIDE
    ///
    /// [BR] BREAK   [EN] ENTER   [CS] CAPS SHIFT   [SS] SYMBOL SHIFT
    ///
    ///       key bits                           key bits
    /// line  b4,  b3,  b2,  b1,      b0   line  b4,  b3,  b2,   b1,      b0
    /// 0xf7 [5], [4], [3], [2],     [1]   0xef [6], [7], [8],  [9],     [0]
    /// 0xfb [T], [R], [E], [W],     [Q]   0xdf [Y], [U], [I],  [O],     [P]
    /// 0xfd [G], [F], [D], [S],     [A]   0xbf [H], [J], [K],  [L], [ENTER]
    /// 0xfe [V], [C], [X], [Z], [SHIFT]   0x7f [B], [N], [M], [SS], [SPACE]
    /// ```
    pub fn read_keyboard(&self, line: u8) -> u8 {
        let mask = !line;
        if mask == 0 {
            return !0;
        }
        let mut res: u8 = !0;
        let mut keymap = self.bits();
        for _ in 0..5 {
            res = res.rotate_left(1);
            let key = keymap as u8;
            if key & mask != 0 {
                res &= !1; // pressed
            }
            keymap >>= 8;
        }
        // eprintln!("keyscan: {:02x} line: {:02x}", res, line);
        res
    }
}
