//! Threshold's elevated helper. Phase 0: a compiling stub.
//!
//! Phase 4 contract, fixed now because it is a security boundary: this binary
//! runs from a "highest privileges" scheduled task, so anything able to trigger
//! that task controls its arguments. It therefore accepts none — instructions
//! come only from C:\ProgramData\Threshold\request.json, and are schema-checked
//! before anything touches the hosts file or the registry.

fn main() {
    std::process::exit(0);
}
