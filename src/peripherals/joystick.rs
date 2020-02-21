//! This module hosts traits for interfacing joystick controllers and joystick device implementations.
pub mod cursor;
pub mod fuller;
pub mod kempston;
pub mod sinclair;

bitflags! {
    /// Bitflags for reading and writing joystick state.
    /// * Bit = 1 is active.
    /// * Bit = 0 is inactive.
    #[derive(Default)]
    pub struct Directions: u8 {
        const UP    = 0b0000_0001;
        const RIGHT = 0b0000_0010;
        const DOWN  = 0b0000_0100;
        const LEFT  = 0b0000_1000;
    }
}

/// An enum for specifying one of 8 possible joystick's directional positions and a center (neutral) state.
pub enum JoyDirection {
    Center,
    Up,
    UpRight,
    Right,
    DownRight,
    Down,
    DownLeft,
    Left,
    UpLeft
}

/// A user input interface for a [JoystickDevice].
pub trait JoystickInterface {
    /// Press or release a fire button. `btn` is the button number for cases when joystick have more than one button.
    fn fire(&mut self, btn: u8, pressed: bool);
    /// Returns a status of a `btn` fire button.
    fn get_fire(&self, btn: u8) -> bool;
    /// Sets joystick direction using bit-flags.
    fn set_directions(&mut self, dir: Directions);
    /// Returns current joystick direction as bit-flags.
    fn get_directions(&self) -> Directions;
    /// Sets joystick direction using an anum.
    #[inline]
    fn direction(&mut self, dir: JoyDirection) {
        self.set_directions(match dir {
            JoyDirection::Center => Directions::empty(),
            JoyDirection::Up => Directions::UP,
            JoyDirection::UpRight => Directions::UP|Directions::RIGHT,
            JoyDirection::Right => Directions::RIGHT,
            JoyDirection::DownRight => Directions::DOWN|Directions::RIGHT,
            JoyDirection::Down => Directions::DOWN,
            JoyDirection::DownLeft => Directions::DOWN|Directions::LEFT,
            JoyDirection::Left => Directions::LEFT,
            JoyDirection::UpLeft => Directions::UP|Directions::LEFT,
        })
    }
    /// Resets joystick to a central (neutral) position.
    #[inline]
    fn center(&mut self) {
        self.set_directions(Directions::empty());
    }
    /// Returns `true` if joystick is in up (forward) position.
    #[inline]
    fn is_up(&self) -> bool {
        self.get_directions().intersects(Directions::UP)
    }
    /// Returns `true` if joystick is in right position.
    #[inline]
    fn is_right(&self) -> bool {
        self.get_directions().intersects(Directions::RIGHT)
    }
    /// Returns `true` if joystick is in left position.
    #[inline]
    fn is_left(&self) -> bool {
        self.get_directions().intersects(Directions::LEFT)
    }
    /// Returns `true` if joystick is in down (backward) position.
    #[inline]
    fn is_down(&self) -> bool {
        self.get_directions().intersects(Directions::DOWN)
    }
    /// Returns `true` if joystick is in a center (neutral) position.
    #[inline]
    fn is_center(&self) -> bool {
        self.get_directions().intersects(Directions::empty())
    }
}

/// A joystick device interface used by the joystick [bus][crate::bus::joystick] device.
pub trait JoystickDevice {
    /// Reads current joystick state as I/O data.
    fn port_read(&self, port: u16) -> u8;
    /// Writes I/O data to a joystick device.
    ///
    /// If a device does not support writes, this method should return `false`.
    /// A default implementation does exactly just that.
    fn port_write(&mut self, _port: u16, _data: u8) -> bool { false }
}
