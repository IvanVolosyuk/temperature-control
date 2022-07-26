//! Generated files are imported from here.
//!
//! For the demonstration we generate descriptors twice, with
//! as pure rust codegen, and with codegen dependent on `protoc` binary.

pub mod generated {
    //include!(concat!(env!("OUT_DIR"), "/generated/mod.rs"));
    include!("generated/mod.rs");
}
