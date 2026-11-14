mod api;
pub mod core;

use nsh_core::prelude::*;

#[native_module("skills-manager-nsh/native")]
fn native(registry: &mut Registry) {
    registry.module::<api::native::AppNative>();
}
