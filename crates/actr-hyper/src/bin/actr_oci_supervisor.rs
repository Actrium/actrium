#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn main() {
    if let Err(error) = actr_hyper::oci::supervisor_main() {
        eprintln!("OCI supervisor: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
fn main() {
    eprintln!("OCI supervision requires Linux x86_64 or aarch64");
    std::process::exit(1);
}
