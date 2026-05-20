#![cfg_attr(docsrs, feature(doc_cfg))]

#[doc(inline)]
pub use cwl_core::*;

#[cfg(feature = "engine")]
#[cfg_attr(docsrs, doc(cfg(feature = "engine")))]
pub mod engine {
    #[doc(inline)]
    pub use cwl_engine::*;
}

#[cfg(feature = "engine")]
#[cfg_attr(docsrs, doc(cfg(feature = "engine")))]
pub mod storage {
    #[doc(inline)]
    pub use cwl_engine_storage::*;
}
