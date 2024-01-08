pub mod compact;
pub(crate) mod compact_capnp;
pub mod hmm;
pub(crate) mod hmm_capnp;
pub mod normal;

pub use crate::compact::CompactModel;
pub use crate::normal::NormalModel;
