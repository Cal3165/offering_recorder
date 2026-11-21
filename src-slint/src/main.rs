#[cfg(not(target_os = "android"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    offering_recorder_slint::run()
}

#[cfg(target_os = "android")]
fn main() {}
