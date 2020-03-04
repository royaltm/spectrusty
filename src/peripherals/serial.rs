//! Serial port related api and emulation of ZX Spectrum's peripheral devices using serial communication.
//!
//! The terminology regarding serial port pins/lines being used in ZX Spectrum's technical documentation:
//!
//! * `RxD` (Receive Data) Transmitted data from Spectrum to the remote device.
//! * `CTS` (Clear to Send) Tells remote station that Spectrum wishes to send data.
//! * `TxD` (Transmit Data) Received data from the remote device.
//! * `DTR` (Data Terminal Ready) Tells Spectrum that remote station wishes to send data.
mod keypad;
mod rs232;

pub use rs232::*;
pub use keypad::*;

/// A type representing a state on one of the `DATA` lines: `RxD` or `TxD`.
///
/// `Space` represents a logical 0 (positive voltage), while `Mark` represents a logical 1 (negative voltage).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum DataState {
    Space = 0,
    Mark = 1
}

/// A type representing a state on one of the `CONTROL` lines: `CTS` or `DTR`.
///
/// `Active` represents a logical 0 (positive voltage), while `Inactive` represents a logical 1 (negative voltage).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum ControlState {
    Active = 0,
    Inactive = 1
}

/// An interface for emulating communication between a ZX Spectrum's hardware port and remote devices.
///
/// Emulators of peripheral devices should implement this trait.
///
/// Methods of this trait are being called by bus devices implementing serial port communication.
pub trait SerialPortDevice {
    type Timestamp: Sized;
    /// A device receives a current `RxD` state from Spectrum and should output a `DTR` line state.
    ///
    /// This method is being called when a `CPU` writes (OUT) to a port and only when the `CTS` line isn't 
    /// changed by the write.
    fn write_data(&mut self, rxd: DataState, timestamp: Self::Timestamp) -> ControlState;
    /// This method is being called once every frame, near the end of it and should return a `DTR` line state.
    fn poll_ready(&mut self, timestamp: Self::Timestamp) -> ControlState;
    /// Receives an updated `CTS` line state.
    ///
    /// This method is being called when a `CPU` writes (OUT) to a port and only when the value of `CTS` changes.
    fn update_cts(&mut self, cts: ControlState, timestamp: Self::Timestamp);
    /// Receives the last `CTS` line state, and should output a `TxD` line state.
    ///
    /// This method is being called when a `CPU` reads (IN) from a port.
    fn read_data(&mut self, timestamp: Self::Timestamp) -> DataState;
    /// Called when the current frame ends to allow emulators to wrap stored timestamps.
    fn end_frame(&mut self, timestamp: Self::Timestamp);
}

impl DataState {
    #[inline]
    pub fn is_space(self) -> bool {
        self == DataState::Space
    }
    #[inline]
    pub fn is_mark(self) -> bool {
        self == DataState::Mark
    }
}

impl ControlState {
    #[inline]
    pub fn is_active(self) -> bool {
        self == ControlState::Active
    }
    #[inline]
    pub fn is_inactive(self) -> bool {
        self == ControlState::Inactive
    }
}

impl From<DataState> for bool {
    #[inline]
    fn from(ds: DataState) -> bool {
        match ds {
            DataState::Space => false,
            DataState::Mark => true,
        }
    }
}

impl From<DataState> for u8 {
    #[inline]
    fn from(ds: DataState) -> u8 {
        ds as u8
    }
}

impl From<bool> for DataState {
    #[inline]
    fn from(flag: bool) -> DataState {
        if flag {
            DataState::Mark
        }
        else {
            DataState::Space
        }
    }
}

impl From<ControlState> for bool {
    #[inline]
    fn from(cs: ControlState) -> bool {
        match cs {
            ControlState::Active => false,
            ControlState::Inactive => true,
        }
    }
}

impl From<bool> for ControlState {
    #[inline]
    fn from(flag: bool) -> ControlState {
        if flag {
            ControlState::Inactive
        }
        else {
            ControlState::Active
        }
    }
}

/// A serial port device that does nothing and provides a constant [ControlState::Inactive] signal
/// on the `DTR` line and a [DataState::Mark] signal on the `TxD` line.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct NullSerialPort<T>(core::marker::PhantomData<T>);

impl<T> SerialPortDevice for NullSerialPort<T> {
    type Timestamp = T;

    #[inline(always)]
    fn write_data(&mut self, _rxd: DataState, _timestamp: Self::Timestamp) -> ControlState {
        ControlState::Inactive
    }
    #[inline(always)]
    fn poll_ready(&mut self, _timestamp: Self::Timestamp) -> ControlState {
        ControlState::Inactive
    }
    #[inline(always)]
    fn update_cts(&mut self, _cts: ControlState, _timestamp: Self::Timestamp) {}
    #[inline(always)]
    fn read_data(&mut self, _timestamp: Self::Timestamp) -> DataState {
        DataState::Mark
    }
    #[inline(always)]
    fn end_frame(&mut self, _timestamp: Self::Timestamp) {}
}
