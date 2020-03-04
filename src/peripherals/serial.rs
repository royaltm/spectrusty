//! Serial port api and implementations.
mod keypad;
mod rs232;

pub use rs232::*;
pub use keypad::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum DataState {
    Space = 0,
    Mark = 1
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum ControlState {
    Active = 0,
    Inactive = 1
}

pub trait SerialPort {
    type Timestamp: Sized;
    /// Resets an underlying device.
    fn reset(&mut self, timestamp: Self::Timestamp);
    /// Receives a current `RxD` state and should output a `DTR` state.
    ///
    /// This method is being called when a CPU writes to a port (OUT) and only when the `CTS` line isn't 
    /// changed by the last write.
    fn write_data(&mut self, rxd: DataState, timestamp: Self::Timestamp) -> ControlState;
    /// This method is being called once every frame, near the end of it and should return a `DTR` state.
    fn poll_ready(&mut self, timestamp: Self::Timestamp) -> ControlState;
    /// Receives an updated `CTS` state.
    ///
    /// This method is being called:
    ///
    /// * when a CPU writes to a port (OUT) and only when the value of `CTS` changes.
    fn update_cts(&mut self, cts: ControlState, timestamp: Self::Timestamp);
    /// Receives a last `CTS` state, and should output a `TxD` state.
    ///
    /// This method is being called:
    ///
    /// * when a CPU reads from a port (IN).
    fn read_data(&mut self, timestamp: Self::Timestamp) -> DataState;
    /// Called when the current frame ends to give opportunity to wrap stored timestamps.
    fn end_frame(&mut self, timestamp: Self::Timestamp);
}

impl DataState {
    #[inline]
    fn is_space(self) -> bool {
        self == DataState::Space
    }
    #[inline]
    fn is_mark(self) -> bool {
        self == DataState::Mark
    }
}

impl ControlState {
    #[inline]
    fn is_active(self) -> bool {
        self == ControlState::Active
    }
    #[inline]
    fn is_inactive(self) -> bool {
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

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct NullSerialPort<T>(core::marker::PhantomData<T>);

impl<T> SerialPort for NullSerialPort<T> {
    type Timestamp = T;

    #[inline(always)]
    fn reset(&mut self, _timestamp: Self::Timestamp) {}
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
